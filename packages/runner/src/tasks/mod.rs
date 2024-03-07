mod connection;
mod converters;
mod events;
mod plane;
mod time;
mod transport_manager;
mod transport_worker;

use std::time::Instant;

use sans_io_runtime::{Controller, Task, TaskGroup, TaskGroupInput, TaskGroupOutputsState, WorkerInner, WorkerInnerInput, WorkerInnerOutput};

use self::{
    connection::{ConnId, ConnectionTask},
    plane::{PlaneCfg, PlaneTask},
    transport_manager::TransportManagerTask,
    transport_worker::TransportWorkerTask,
};

pub type SdnController = Controller<SdnExtIn, SdnExtOut, SdnSpawnCfg, SdnChannel, SdnEvent, 128>;

pub type SdnExtIn = ();
pub type SdnExtOut = ();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SdnChannel {
    Plane,
    TransportManager,
    Connection(ConnId),
    TransportWorker(u16),
}

#[derive(Debug, Clone)]
pub enum SdnEvent {
    Plane(plane::EventIn),
    Connection(connection::EventIn),
    TransportManager(transport_manager::EventIn),
    TransportWorker(transport_worker::EventIn),
}

pub struct SdnInnerCfg {
    pub behaviours: Option<Vec<()>>,
}

pub struct SdnSpawnCfg {
    cfg: connection::SpawnCfg,
}

pub struct SdnWorkerInner {
    worker: u16,
    plane: Option<PlaneTask>,
    transport_manager: Option<TransportManagerTask>,
    transport_worker: TransportWorkerTask,
    connections: TaskGroup<connection::ChannelIn, connection::ChannelOut, connection::EventIn, connection::EventOut, ConnectionTask, 128>,
    group_state: TaskGroupOutputsState<4>,
    last_input_group: Option<u16>,
}

impl SdnWorkerInner {}

impl WorkerInner<SdnExtIn, SdnExtOut, SdnChannel, SdnEvent, SdnInnerCfg, SdnSpawnCfg> for SdnWorkerInner {
    fn build(worker: u16, cfg: SdnInnerCfg) -> Self {
        let core = cfg.behaviours.is_some();
        Self {
            worker,
            plane: if core {
                Some(PlaneTask::build(PlaneCfg {}))
            } else {
                None
            },
            transport_manager: if core {
                Some(TransportManagerTask::build())
            } else {
                None
            },
            transport_worker: TransportWorkerTask::build(),
            connections: TaskGroup::new(worker),
            group_state: TaskGroupOutputsState::default(),
            last_input_group: None,
        }
    }

    fn worker_index(&self) -> u16 {
        self.worker
    }

    fn tasks(&self) -> usize {
        self.connections.tasks()
            + 1
            + if self.plane.is_some() {
                2
            } else {
                0
            }
    }

    fn spawn(&mut self, _now: Instant, cfg: SdnSpawnCfg) {
        self.connections.add_task(ConnectionTask::build(cfg.cfg));
    }

    fn on_tick<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        let gs = &mut self.group_state;
        self.last_input_group = None;
        loop {
            match gs.current()? {
                PlaneTask::TYPE => {
                    if let Some(out) = gs.process(self.plane.as_mut().map(|p| p.on_tick(now))).flatten() {
                        self.last_input_group = Some(PlaneTask::TYPE);
                        return Some(converters::plane::convert_output(self.worker, out));
                    }
                }
                TransportManagerTask::TYPE => {
                    if let Some(out) = gs.process(self.transport_manager.as_mut().map(|p| p.on_tick(now))).flatten() {
                        self.last_input_group = Some(TransportManagerTask::TYPE);
                        return Some(converters::transport_manager::convert_output(self.worker, now, out));
                    }
                }
                TransportWorkerTask::TYPE => {
                    if let Some(out) = gs.process(self.transport_worker.on_tick(now)) {
                        self.last_input_group = Some(TransportWorkerTask::TYPE);
                        return Some(converters::transport_worker::convert_output(self.worker, out));
                    }
                }
                ConnectionTask::TYPE => {
                    if let Some(out) = gs.process(self.connections.on_tick(now)) {
                        self.last_input_group = Some(ConnectionTask::TYPE);
                        return Some(converters::connection::convert_output(self.worker, out.0, out.1));
                    }
                }
                _ => panic!("Invalid task type"),
            }
        }
    }

    fn on_event<'a>(&mut self, now: Instant, event: WorkerInnerInput<'a, SdnExtIn, SdnChannel, SdnEvent>) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        match event {
            WorkerInnerInput::Ext(_) => None,
            WorkerInnerInput::Task(owner, event) => match owner.group_id().expect("Should have group_id") {
                PlaneTask::TYPE => {
                    let plane = self.plane.as_mut()?;
                    let out = plane.on_event(now, converters::plane::convert_input(event))?;
                    self.last_input_group = Some(PlaneTask::TYPE);
                    return Some(converters::plane::convert_output(self.worker, out));
                }
                TransportManagerTask::TYPE => {
                    let tm = self.transport_manager.as_mut()?;
                    let out = tm.on_event(now, converters::transport_manager::convert_input(event))?;
                    self.last_input_group = Some(TransportManagerTask::TYPE);
                    return Some(converters::transport_manager::convert_output(self.worker, now, out));
                }
                TransportWorkerTask::TYPE => {
                    let tw = &mut self.transport_worker;
                    let out = tw.on_event(now, converters::transport_worker::convert_input(event))?;
                    self.last_input_group = Some(TransportWorkerTask::TYPE);
                    return Some(converters::transport_worker::convert_output(self.worker, out));
                }
                ConnectionTask::TYPE => {
                    let conns = &mut self.connections;
                    let input = converters::connection::convert_input(event);
                    let out = conns.on_event(now, TaskGroupInput(owner, input))?;
                    self.last_input_group = Some(ConnectionTask::TYPE);
                    return Some(converters::connection::convert_output(self.worker, out.0, out.1));
                }
                _ => panic!("Invalid task type"),
            },
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        match self.last_input_group? {
            PlaneTask::TYPE => {
                let out = self.plane.as_mut().map(|p| p.pop_output(now)).flatten()?;
                return Some(converters::plane::convert_output(self.worker, out));
            }
            TransportManagerTask::TYPE => {
                let out = self.transport_manager.as_mut().map(|p| p.pop_output(now)).flatten()?;
                return Some(converters::transport_manager::convert_output(self.worker, now, out));
            }
            TransportWorkerTask::TYPE => {
                let out = self.transport_worker.pop_output(now)?;
                return Some(converters::transport_worker::convert_output(self.worker, out));
            }
            ConnectionTask::TYPE => {
                let out = self.connections.pop_output(now)?;
                return Some(converters::connection::convert_output(self.worker, out.0, out.1));
            }
            _ => panic!("Invalid task type"),
        }
    }
}

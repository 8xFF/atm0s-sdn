mod connection;
mod events;
mod plane;
mod time;
mod transport_manager;
mod transport_worker;

use std::time::Instant;

use sans_io_runtime::{bus::BusEvent, Controller, Owner, Task, TaskGroup, TaskGroupOutputsState, TaskInput, TaskOutput, WorkerInner, WorkerInnerInput, WorkerInnerOutput};

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

impl SdnWorkerInner {
    
}

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

    fn spawn(&mut self, now: Instant, cfg: SdnSpawnCfg) {
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
                        return Some(convert_plane_output(self.worker, out));
                    }
                }
                TransportManagerTask::TYPE => {
                    if let Some(out) = gs.process(self.transport_manager.as_mut().map(|p| p.on_tick(now))).flatten() {
                        self.last_input_group = Some(TransportManagerTask::TYPE);
                        return Some(convert_transport_manager_output(self.worker, out));
                    }
                }
                TransportWorkerTask::TYPE => {}
                ConnectionTask::TYPE => {}
                _ => panic!("Invalid task type"),
            }
        }
    }

    fn on_event<'a>(&mut self, now: Instant, event: WorkerInnerInput<'a, SdnExtIn, SdnChannel, SdnEvent>) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        match event {
            WorkerInnerInput::Ext(_) => None,
            WorkerInnerInput::Task(owner, event) => {
                match owner.group_id().expect("Should have group_id") {
                    PlaneTask::TYPE => {
                        let plane = self.plane.as_mut()?;
                        let out = plane.on_input(now, convert_plane_input(event))?;
                        self.last_input_group = Some(PlaneTask::TYPE);
                        return Some(convert_plane_output(self.worker, out));
                    },
                    TransportManagerTask::TYPE => {
                        let tm = self.transport_manager.as_mut()?;
                        let out = tm.on_input(now, convert_transport_manager_input(event))?;
                        self.last_input_group = Some(TransportManagerTask::TYPE);
                        return Some(convert_transport_manager_output(self.worker, out));
                    },
                    TransportWorkerTask::TYPE => None,
                    ConnectionTask::TYPE => None,
                    _ => panic!("Invalid task type"),
                }
            }
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        match self.last_input_group? {
            PlaneTask::TYPE => {
                let out = self.plane.as_mut().map(|p| p.pop_output(now)).flatten()?;
                return Some(convert_plane_output(self.worker, out));
            }
            TransportManagerTask::TYPE => {
                let out = self.plane.as_mut().map(|p| p.pop_output(now)).flatten()?;
                return Some(convert_plane_output(self.worker, out));
            },
            TransportWorkerTask::TYPE => None,
            ConnectionTask::TYPE => None,
            _ => panic!("Invalid task type"),
        
        }
    }
}


/// 
/// 
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
/// 
fn convert_plane_input<'a>(event: TaskInput<'a, SdnChannel, SdnEvent>) -> TaskInput<'a, plane::ChannelIn, plane::EventIn> {
    if let TaskInput::Bus(_, SdnEvent::Plane(event)) = event {
        TaskInput::Bus((), event)
    } else {
        panic!("Invalid input type from Plane")
    }
}


///
/// 
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
/// 
fn convert_plane_output<'a>(worker: u16, event: TaskOutput<plane::ChannelIn, plane::ChannelOut, plane::EventOut>) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg> {
    match event {
        TaskOutput::Bus(BusEvent::ChannelSubscribe(_)) => {
            WorkerInnerOutput::Task(
                Owner::group(worker, PlaneTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::Plane)),
            )
        },
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(_)) => {
            WorkerInnerOutput::Task(
                Owner::group(worker, PlaneTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::Plane)),
            )
        },
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, event)) => match event {
            plane::EventOut::ToHandlerBus(conn, service, data) => {
                WorkerInnerOutput::Task(
                    Owner::group(worker, ConnectionTask::TYPE),
                    TaskOutput::Bus(BusEvent::ChannelPublish(
                        SdnChannel::Connection(conn),
                        safe,
                        SdnEvent::Connection(connection::EventIn::Bus(service, events::BusEvent::FromBehavior(data))),
                    )),
                )
            },
            plane::EventOut::ConnectTo(addr) => {
                WorkerInnerOutput::Task(
                    Owner::group(worker, ConnectionTask::TYPE),
                    TaskOutput::Bus(BusEvent::ChannelPublish(
                        SdnChannel::TransportManager,
                        safe,
                        SdnEvent::TransportManager(transport_manager::EventIn::ConnectTo(addr)),
                    )),
                )
            },
            plane::EventOut::SpawnConnection(cfg) => {
                WorkerInnerOutput::Spawn(SdnSpawnCfg {cfg})
            }
        },
        _ => panic!("Invalid output type from Plane")
    }
}

/// 
/// 
/// This function will convert the input from SDN into Plane task input.
/// It only accept bus events from the SDN task.
/// 
fn convert_transport_manager_input<'a>(event: TaskInput<'a, SdnChannel, SdnEvent>) -> TaskInput<'a, transport_manager::ChannelIn, transport_manager::EventIn> {
    if let TaskInput::Bus(_, SdnEvent::TransportManager(event)) = event {
        TaskInput::Bus((), event)
    } else {
        panic!("Invalid input type from Plane")
    }
}

///
/// 
/// This function will convert the output from the Plane task into the output for the SDN task.
/// It only accept bus events from the Plane task.
/// 
fn convert_transport_manager_output<'a>(worker: u16, event: TaskOutput<transport_manager::ChannelIn, transport_manager::ChannelOut, transport_manager::EventOut>) -> WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg> {
    match event {
        TaskOutput::Bus(BusEvent::ChannelSubscribe(_)) => {
            WorkerInnerOutput::Task(
                Owner::group(worker, TransportManagerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(SdnChannel::TransportManager)),
            )
        },
        TaskOutput::Bus(BusEvent::ChannelUnsubscribe(_)) => {
            WorkerInnerOutput::Task(
                Owner::group(worker, TransportManagerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelUnsubscribe(SdnChannel::TransportManager)),
            )
        },
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, transport_manager::EventOut::Transport(event))) => {
            WorkerInnerOutput::Task(
                Owner::group(worker, TransportWorkerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::Plane,
                    safe,
                    SdnEvent::Plane(plane::EventIn::Transport(event)),
                )),
            )
        }
        TaskOutput::Bus(BusEvent::ChannelPublish(_, safe, transport_manager::EventOut::Worker(event))) => {
            WorkerInnerOutput::Task(
                Owner::group(worker, TransportWorkerTask::TYPE),
                TaskOutput::Bus(BusEvent::ChannelPublish(
                    SdnChannel::TransportWorker(worker),
                    safe,
                    SdnEvent::TransportWorker(event),
                )),
            )
        }
        _ => panic!("Invalid output type from Plane")
    }
}
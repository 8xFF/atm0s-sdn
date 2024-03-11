mod controller_plane;
mod data_plane;
mod event_convert;

use std::time::Instant;

use atm0s_sdn_identity::NodeAddr;
use sans_io_runtime::{Controller, Task, TaskGroupOutputsState, TaskInput, WorkerInner, WorkerInnerInput, WorkerInnerOutput};

use self::{
    controller_plane::{ControllerPlaneCfg, ControllerPlaneTask},
    data_plane::DataPlaneTask,
};

pub type SdnController = Controller<SdnExtIn, SdnExtOut, SdnSpawnCfg, SdnChannel, SdnEvent, 128>;

#[derive(Debug, Clone)]
pub enum SdnExtIn {
    ConnectTo(NodeAddr),
}

pub type SdnExtOut = ();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SdnChannel {
    ControllerPlane(controller_plane::ChannelIn),
    DataPlane(data_plane::ChannelIn),
}

#[derive(Debug, Clone)]
pub enum SdnEvent {
    ControllerPlane(controller_plane::EventIn),
    DataPlane(data_plane::EventIn),
}

pub struct ControllerCfg {
    pub password: String,
    pub tick_ms: u64,
}

pub struct SdnInnerCfg {
    pub node_id: u32,
    pub udp_port: u16,
    pub controller: Option<ControllerCfg>,
}

pub struct SdnSpawnCfg {}

pub struct SdnWorkerInner {
    worker: u16,
    controller: Option<ControllerPlaneTask>,
    data: DataPlaneTask,
    group_state: TaskGroupOutputsState<2>,
    last_input_group: Option<u16>,
}

impl SdnWorkerInner {}

impl WorkerInner<SdnExtIn, SdnExtOut, SdnChannel, SdnEvent, SdnInnerCfg, SdnSpawnCfg> for SdnWorkerInner {
    fn build(worker: u16, cfg: SdnInnerCfg) -> Self {
        if let Some(controller) = cfg.controller {
            Self {
                worker,
                controller: Some(ControllerPlaneTask::build(ControllerPlaneCfg {
                    node_id: cfg.node_id,
                    tick_ms: controller.tick_ms,
                })),
                data: DataPlaneTask::build(worker, cfg.udp_port),
                group_state: TaskGroupOutputsState::default(),
                last_input_group: None,
            }
        } else {
            Self {
                worker,
                controller: None,
                data: DataPlaneTask::build(worker, cfg.udp_port),
                group_state: TaskGroupOutputsState::default(),
                last_input_group: None,
            }
        }
    }

    fn worker_index(&self) -> u16 {
        self.worker
    }

    fn tasks(&self) -> usize {
        1 + if self.controller.is_some() {
            1
        } else {
            0
        }
    }

    fn spawn(&mut self, _now: Instant, cfg: SdnSpawnCfg) {
        todo!("Spawn not implemented")
    }

    fn on_tick<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        self.last_input_group = None;
        let gs = &mut self.group_state;
        loop {
            match gs.current()? {
                ControllerPlaneTask::TYPE => {
                    if let Some(out) = gs.process(self.controller.as_mut().map(|c| c.on_tick(now)).flatten()) {
                        self.last_input_group = Some(ControllerPlaneTask::TYPE);
                        return Some(event_convert::controller_plane::convert_output(self.worker, out));
                    }
                }
                DataPlaneTask::TYPE => {
                    if let Some(out) = gs.process(self.data.on_tick(now)) {
                        self.last_input_group = Some(DataPlaneTask::TYPE);
                        return Some(event_convert::data_plane::convert_output(self.worker, out));
                    }
                }
                _ => panic!("unknown task type"),
            }
        }
    }

    fn on_event<'a>(&mut self, now: Instant, event: WorkerInnerInput<'a, SdnExtIn, SdnChannel, SdnEvent>) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        self.last_input_group = None;
        match event {
            WorkerInnerInput::Task(owner, event) => match owner.group_id()? {
                ControllerPlaneTask::TYPE => {
                    let event = event_convert::controller_plane::convert_input(event);
                    let out = self.controller.as_mut().map(|c| c.on_event(now, event)).flatten()?;
                    self.last_input_group = Some(ControllerPlaneTask::TYPE);
                    Some(event_convert::controller_plane::convert_output(self.worker, out))
                }
                DataPlaneTask::TYPE => {
                    let event = event_convert::data_plane::convert_input(event);
                    let out = self.data.on_event(now, event)?;
                    self.last_input_group = Some(DataPlaneTask::TYPE);
                    Some(event_convert::data_plane::convert_output(self.worker, out))
                }
                _ => panic!("unknown task type"),
            },
            WorkerInnerInput::Ext(ext) => match ext {
                SdnExtIn::ConnectTo(addr) => {
                    log::info!("Connect to {}", addr);
                    let event = TaskInput::Bus((), controller_plane::EventIn::ConnectTo(addr));
                    let out = self.controller.as_mut().map(|c| c.on_event(now, event)).flatten()?;
                    self.last_input_group = Some(ControllerPlaneTask::TYPE);
                    Some(event_convert::controller_plane::convert_output(self.worker, out))
                }
            },
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        match self.last_input_group? {
            ControllerPlaneTask::TYPE => {
                let out = self.controller.as_mut().map(|c| c.pop_output(now)).flatten()?;
                Some(event_convert::controller_plane::convert_output(self.worker, out))
            }
            DataPlaneTask::TYPE => {
                let out = self.data.pop_output(now)?;
                Some(event_convert::data_plane::convert_output(self.worker, out))
            }
            _ => panic!("unknown task type"),
        }
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent, SdnSpawnCfg>> {
        None
    }
}

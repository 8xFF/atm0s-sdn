mod controller_plane;
mod data_plane;
mod event_convert;

use std::{fmt::Debug, time::Instant};

use atm0s_sdn_network::{ExtIn, ExtOut};
use sans_io_runtime::{Controller, Task, TaskGroupOutputsState, TaskInput, TaskOutput, WorkerInner, WorkerInnerInput, WorkerInnerOutput};

use self::{
    controller_plane::{ControllerPlaneCfg, ControllerPlaneTask},
    data_plane::{DataPlaneCfg, DataPlaneTask},
};

pub type SdnController<TC, TW> = Controller<SdnExtIn, SdnExtOut, SdnSpawnCfg, SdnChannel, SdnEvent<TC, TW>, 1024>;

pub type SdnExtIn = ExtIn;
pub type SdnExtOut = ExtOut;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SdnChannel {
    ControllerPlane(controller_plane::ChannelIn),
    DataPlane(data_plane::ChannelIn),
}

#[derive(Debug, Clone)]
pub enum SdnEvent<TC, TW> {
    ControllerPlane(controller_plane::EventIn<TC>),
    DataPlane(data_plane::EventIn<TW>),
}

pub struct ControllerCfg {
    pub password: String,
    pub tick_ms: u64,
    // pub services: Vec<Box<dyn atm0s_sdn_network::controller_plane::Service>>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_device: sans_io_runtime::backend::tun::TunDevice,
}

pub struct SdnInnerCfg {
    pub node_id: u32,
    pub udp_port: u16,
    pub controller: Option<ControllerCfg>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_fd: sans_io_runtime::backend::tun::TunFd,
}

pub struct SdnSpawnCfg {}

enum State {
    Running,
    Shutdowning,
    Shutdowned,
}

pub struct SdnWorkerInner<TC, TW> {
    worker: u16,
    controller: Option<ControllerPlaneTask<TC, TW>>,
    data: DataPlaneTask<TC, TW>,
    group_state: TaskGroupOutputsState<2>,
    last_input_group: Option<u16>,
    state: State,
}

impl<TC: Debug, TW: Debug> SdnWorkerInner<TC, TW> {
    fn convert_controller_output<'a>(
        &mut self,
        now: Instant,
        event: TaskOutput<'a, ExtOut, controller_plane::ChannelIn, controller_plane::ChannelOut, controller_plane::EventOut<TW>>,
    ) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        match event {
            TaskOutput::Destroy => {
                self.state = State::Shutdowned;
                log::info!("Controller plane task destroyed => will destroy data plane task");
                Some(event_convert::data_plane::convert_output(self.worker, self.data.shutdown(now)?))
            }
            _ => Some(event_convert::controller_plane::convert_output(self.worker, event)),
        }
    }
}

impl<TC: Debug, TW: Debug> WorkerInner<SdnExtIn, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnInnerCfg, SdnSpawnCfg> for SdnWorkerInner<TC, TW> {
    fn build(worker: u16, cfg: SdnInnerCfg) -> Self {
        if let Some(controller) = cfg.controller {
            Self {
                worker,
                controller: Some(ControllerPlaneTask::build(ControllerPlaneCfg {
                    node_id: cfg.node_id,
                    tick_ms: controller.tick_ms,
                    // services: controller.services,
                    #[cfg(feature = "vpn")]
                    vpn_tun_device: controller.vpn_tun_device,
                })),
                data: DataPlaneTask::build(DataPlaneCfg {
                    worker,
                    node_id: cfg.node_id,
                    port: cfg.udp_port,
                    #[cfg(feature = "vpn")]
                    vpn_tun_fd: cfg.vpn_tun_fd,
                }),
                group_state: TaskGroupOutputsState::default(),
                last_input_group: None,
                state: State::Running,
            }
        } else {
            Self {
                worker,
                controller: None,
                data: DataPlaneTask::build(DataPlaneCfg {
                    worker,
                    node_id: cfg.node_id,
                    port: cfg.udp_port,
                    #[cfg(feature = "vpn")]
                    vpn_tun_fd: cfg.vpn_tun_fd,
                }),
                group_state: TaskGroupOutputsState::default(),
                last_input_group: None,
                state: State::Running,
            }
        }
    }

    fn worker_index(&self) -> u16 {
        self.worker
    }

    fn tasks(&self) -> usize {
        match self.state {
            State::Running | State::Shutdowning => {
                1 + if self.controller.is_some() {
                    1
                } else {
                    0
                }
            }
            State::Shutdowned => 0,
        }
    }

    fn spawn(&mut self, _now: Instant, _cfg: SdnSpawnCfg) {
        todo!("Spawn not implemented")
    }

    fn on_tick<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        self.last_input_group = None;
        let gs = &mut self.group_state;
        loop {
            match gs.current()? {
                ControllerPlaneTask::<(), ()>::TYPE => {
                    if let Some(out) = gs.process(self.controller.as_mut().map(|c| c.on_tick(now)).flatten()) {
                        self.last_input_group = Some(ControllerPlaneTask::<(), ()>::TYPE);
                        return self.convert_controller_output(now, out);
                    }
                }
                DataPlaneTask::<(), ()>::TYPE => {
                    if let Some(out) = gs.process(self.data.on_tick(now)) {
                        self.last_input_group = Some(DataPlaneTask::<(), ()>::TYPE);
                        return Some(event_convert::data_plane::convert_output(self.worker, out));
                    }
                }
                _ => panic!("unknown task type"),
            }
        }
    }

    fn on_event<'a>(
        &mut self,
        now: Instant,
        event: WorkerInnerInput<'a, SdnExtIn, SdnChannel, SdnEvent<TC, TW>>,
    ) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        self.last_input_group = None;
        match event {
            WorkerInnerInput::Task(owner, event) => match owner.group_id()? {
                ControllerPlaneTask::<(), ()>::TYPE => {
                    let event = event_convert::controller_plane::convert_input(event);
                    let out = self.controller.as_mut().map(|c| c.on_event(now, event)).flatten()?;
                    self.last_input_group = Some(ControllerPlaneTask::<(), ()>::TYPE);
                    self.convert_controller_output(now, out)
                }
                DataPlaneTask::<(), ()>::TYPE => {
                    let event = event_convert::data_plane::convert_input(event);
                    let out = self.data.on_event(now, event)?;
                    self.last_input_group = Some(DataPlaneTask::<(), ()>::TYPE);
                    Some(event_convert::data_plane::convert_output(self.worker, out))
                }
                _ => panic!("unknown task type"),
            },
            WorkerInnerInput::Ext(ext) => {
                let out = self.controller.as_mut().map(|c| c.on_event(now, TaskInput::Ext(ext))).flatten()?;
                self.last_input_group = Some(ControllerPlaneTask::<(), ()>::TYPE);
                Some(event_convert::controller_plane::convert_output(self.worker, out))
            }
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        match self.last_input_group? {
            ControllerPlaneTask::<(), ()>::TYPE => {
                let out = self.controller.as_mut().map(|c| c.pop_output(now)).flatten()?;
                self.convert_controller_output(now, out)
            }
            DataPlaneTask::<(), ()>::TYPE => {
                let out = self.data.pop_output(now)?;
                Some(event_convert::data_plane::convert_output(self.worker, out))
            }
            _ => panic!("unknown task type"),
        }
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        if !matches!(self.state, State::Running) {
            return None;
        }

        if let Some(controller) = &mut self.controller {
            self.state = State::Shutdowning;
            let out = controller.shutdown(now)?;
            self.convert_controller_output(now, out)
        } else {
            self.state = State::Shutdowned;
            Some(event_convert::data_plane::convert_output(self.worker, self.data.shutdown(now)?))
        }
    }
}

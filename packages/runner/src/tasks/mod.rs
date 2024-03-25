mod controller_plane;
mod data_plane;
mod event_convert;

use std::{fmt::Debug, sync::Arc, time::Instant};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::{
    base::ServiceBuilder,
    features::{FeaturesControl, FeaturesEvent},
    san_io_utils::TasksSwitcher,
    ExtIn, ExtOut,
};
use atm0s_sdn_router::shadow::ShadowRouterHistory;
use sans_io_runtime::{Controller, Task, TaskInput, TaskOutput, WorkerInner, WorkerInnerInput, WorkerInnerOutput};

pub use self::data_plane::history::DataWorkerHistory;
use self::{
    controller_plane::{ControllerPlaneCfg, ControllerPlaneTask},
    data_plane::{DataPlaneCfg, DataPlaneTask},
};

pub type SdnController<SC, SE, TC, TW> = Controller<SdnExtIn<SC>, SdnExtOut<SE>, SdnSpawnCfg, SdnChannel, SdnEvent<TC, TW>, 1024>;

pub type SdnExtIn<SC> = ExtIn<SC>;
pub type SdnExtOut<SE> = ExtOut<SE>;

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
    pub session: u64,
    pub password: String,
    pub tick_ms: u64,
    #[cfg(feature = "vpn")]
    pub vpn_tun_device: Option<sans_io_runtime::backend::tun::TunDevice>,
}

pub struct SdnInnerCfg<SC, SE, TC, TW> {
    pub node_id: NodeId,
    pub udp_port: u16,
    pub controller: Option<ControllerCfg>,
    pub services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>,
    pub history: Arc<dyn ShadowRouterHistory>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_fd: Option<sans_io_runtime::backend::tun::TunFd>,
}

pub struct SdnSpawnCfg {}

enum State {
    Running,
    Shutdowning,
    Shutdowned,
}

pub struct SdnWorkerInner<SC, SE, TC, TW> {
    worker: u16,
    controller: Option<ControllerPlaneTask<SC, SE, TC, TW>>,
    data: DataPlaneTask<SC, SE, TC, TW>,
    switcher: TasksSwitcher<u16, 2>,
    state: State,
}

impl<SC, SE, TC: Debug, TW: Debug> SdnWorkerInner<SC, SE, TC, TW> {
    fn convert_controller_output<'a>(
        &mut self,
        now: Instant,
        event: TaskOutput<'a, ExtOut<SE>, controller_plane::ChannelIn, controller_plane::ChannelOut, controller_plane::EventOut<TW>>,
    ) -> Option<WorkerInnerOutput<'a, SdnExtOut<SE>, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
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

impl<SC, SE, TC: Debug, TW: Debug> WorkerInner<SdnExtIn<SC>, SdnExtOut<SE>, SdnChannel, SdnEvent<TC, TW>, SdnInnerCfg<SC, SE, TC, TW>, SdnSpawnCfg> for SdnWorkerInner<SC, SE, TC, TW> {
    fn build(worker: u16, cfg: SdnInnerCfg<SC, SE, TC, TW>) -> Self {
        if let Some(controller) = cfg.controller {
            log::info!("Create controller worker");
            Self {
                worker,
                controller: Some(ControllerPlaneTask::build(ControllerPlaneCfg {
                    node_id: cfg.node_id,
                    session: controller.session,
                    tick_ms: controller.tick_ms,
                    services: cfg.services.clone(),
                    #[cfg(feature = "vpn")]
                    vpn_tun_device: controller.vpn_tun_device,
                })),
                data: DataPlaneTask::build(DataPlaneCfg {
                    worker,
                    node_id: cfg.node_id,
                    port: cfg.udp_port,
                    services: cfg.services,
                    history: cfg.history,
                    #[cfg(feature = "vpn")]
                    vpn_tun_fd: cfg.vpn_tun_fd,
                }),
                switcher: TasksSwitcher::default(),
                state: State::Running,
            }
        } else {
            log::info!("Create data only worker");
            Self {
                worker,
                controller: None,
                data: DataPlaneTask::build(DataPlaneCfg {
                    worker,
                    node_id: cfg.node_id,
                    port: cfg.udp_port,
                    services: cfg.services,
                    history: cfg.history,
                    #[cfg(feature = "vpn")]
                    vpn_tun_fd: cfg.vpn_tun_fd,
                }),
                switcher: TasksSwitcher::default(),
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

    fn on_tick<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut<SE>, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        self.switcher.push_all();
        let s = &mut self.switcher;
        loop {
            match s.current()? as u16 {
                ControllerPlaneTask::<(), (), (), ()>::TYPE => {
                    if let Some(out) = s.process(self.controller.as_mut().map(|c| c.on_tick(now)).flatten()) {
                        s.push_last(ControllerPlaneTask::<(), (), (), ()>::TYPE);
                        return self.convert_controller_output(now, out);
                    }
                }
                DataPlaneTask::<(), (), (), ()>::TYPE => {
                    if let Some(out) = s.process(self.data.on_tick(now)) {
                        s.push_last(DataPlaneTask::<(), (), (), ()>::TYPE);
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
        event: WorkerInnerInput<'a, SdnExtIn<SC>, SdnChannel, SdnEvent<TC, TW>>,
    ) -> Option<WorkerInnerOutput<'a, SdnExtOut<SE>, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        match event {
            WorkerInnerInput::Task(owner, event) => match owner.group_id()? {
                ControllerPlaneTask::<(), (), (), ()>::TYPE => {
                    let event = event_convert::controller_plane::convert_input(event);
                    let out = self.controller.as_mut().map(|c| c.on_event(now, event)).flatten()?;
                    self.switcher.push_last(ControllerPlaneTask::<(), (), (), ()>::TYPE);
                    self.convert_controller_output(now, out)
                }
                DataPlaneTask::<(), (), (), ()>::TYPE => {
                    let event = event_convert::data_plane::convert_input(event);
                    let out = self.data.on_event(now, event)?;
                    self.switcher.push_last(DataPlaneTask::<(), (), (), ()>::TYPE);
                    Some(event_convert::data_plane::convert_output(self.worker, out))
                }
                _ => panic!("unknown task type"),
            },
            WorkerInnerInput::Ext(ext) => {
                let out = self.controller.as_mut().map(|c| c.on_event(now, TaskInput::Ext(ext))).flatten()?;
                self.switcher.push_last(ControllerPlaneTask::<(), (), (), ()>::TYPE);
                Some(event_convert::controller_plane::convert_output(self.worker, out))
            }
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut<SE>, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
        while let Some(current) = self.switcher.current() {
            match current {
                ControllerPlaneTask::<(), (), (), ()>::TYPE => {
                    let out = self.controller.as_mut().map(|c| c.pop_output(now)).flatten();
                    if let Some(out) = self.switcher.process(out) {
                        let out = self.convert_controller_output(now, out);
                        if out.is_some() {
                            return out;
                        }
                    }
                }
                DataPlaneTask::<(), (), (), ()>::TYPE => {
                    let out = self.data.pop_output(now);
                    if let Some(out) = self.switcher.process(out) {
                        return Some(event_convert::data_plane::convert_output(self.worker, out));
                    }
                }
                _ => panic!("unknown task type"),
            }
        }
        None
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<WorkerInnerOutput<'a, SdnExtOut<SE>, SdnChannel, SdnEvent<TC, TW>, SdnSpawnCfg>> {
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

use std::{fmt::Debug, hash::Hash};

use atm0s_sdn_identity::NodeId;
use sans_io_runtime::{TaskSwitcher, TaskSwitcherBranch, TaskSwitcherChild};

use crate::{
    controller_plane::{self, ControllerPlane, ControllerPlaneCfg},
    data_plane::{self, CrossWorker, DataPlane, DataPlaneCfg, NetInput, NetOutput},
    ExtIn, ExtOut, LogicControl, LogicEvent, LogicEventDest,
};

#[derive(Debug, Clone)]
pub enum SdnWorkerBusEvent<UserData, SC, SE, TC, TW> {
    Control(LogicControl<UserData, SC, SE, TC>),
    Workers(LogicEvent<UserData, SE, TW>),
    Worker(u16, CrossWorker<UserData, SE>),
}

pub enum SdnWorkerInput<UserData, SC, SE, TC, TW> {
    Ext(ExtIn<UserData, SC>),
    ExtWorker(ExtIn<UserData, SC>),
    Net(NetInput),
    Bus(SdnWorkerBusEvent<UserData, SC, SE, TC, TW>),
}

#[derive(Debug)]
pub enum SdnWorkerOutput<UserData, SC, SE, TC, TW> {
    Ext(ExtOut<UserData, SE>),
    ExtWorker(ExtOut<UserData, SE>),
    Net(NetOutput),
    Bus(SdnWorkerBusEvent<UserData, SC, SE, TC, TW>),
    OnResourceEmpty,
    Continue,
}

#[derive(Debug, num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
#[repr(usize)]
pub enum TaskType {
    Controller = 0,
    Data = 1,
}

pub struct SdnWorkerCfg<UserData, SC, SE, TC, TW> {
    pub node_id: NodeId,
    pub tick_ms: u64,
    pub controller: Option<ControllerPlaneCfg<UserData, SC, SE, TC, TW>>,
    pub data: DataPlaneCfg<UserData, SC, SE, TC, TW>,
}

pub struct SdnWorker<UserData, SC, SE, TC, TW> {
    tick_ms: u64,
    #[allow(clippy::type_complexity)]
    controller: Option<TaskSwitcherBranch<ControllerPlane<UserData, SC, SE, TC, TW>, controller_plane::Output<UserData, SE, TW>>>,
    #[allow(clippy::type_complexity)]
    data: TaskSwitcherBranch<DataPlane<UserData, SC, SE, TC, TW>, data_plane::Output<UserData, SC, SE, TC>>,
    shutdown: bool,
    switcher: TaskSwitcher,
    last_tick: Option<u64>,
}

impl<UserData, SC: Debug, SE: Debug, TC: Debug, TW: Debug> SdnWorker<UserData, SC, SE, TC, TW>
where
    UserData: 'static + Eq + Copy + Debug + Hash,
{
    pub fn new(cfg: SdnWorkerCfg<UserData, SC, SE, TC, TW>) -> Self {
        Self {
            tick_ms: cfg.tick_ms,
            controller: cfg
                .controller
                .map(|controller| TaskSwitcherBranch::new(ControllerPlane::new(cfg.node_id, controller), TaskType::Controller)),
            data: TaskSwitcherBranch::new(DataPlane::new(cfg.node_id, cfg.data), TaskType::Data),
            shutdown: false,
            switcher: TaskSwitcher::new(2),
            last_tick: None,
        }
    }

    pub fn tasks(&self) -> usize {
        1 + self.controller.as_ref().map_or(0, |_| 1)
    }

    pub fn is_empty(&self) -> bool {
        self.shutdown && self.controller.as_ref().map_or(true, |c| c.is_empty()) && self.data.is_empty()
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        if let Some(last_tick) = self.last_tick {
            if now_ms < last_tick + self.tick_ms {
                return;
            }
        }
        self.last_tick = Some(now_ms);

        self.data.input(&mut self.switcher).on_tick(now_ms);
        if let Some(controller) = &mut self.controller {
            controller.input(&mut self.switcher).on_tick(now_ms);
        }
    }

    pub fn on_event(&mut self, now_ms: u64, input: SdnWorkerInput<UserData, SC, SE, TC, TW>) {
        match input {
            SdnWorkerInput::Ext(ext) => {
                let controller = self.controller.as_mut().expect("Should have controller");
                controller.input(&mut self.switcher).on_event(now_ms, controller_plane::Input::Ext(ext));
            }
            SdnWorkerInput::ExtWorker(ext) => {
                self.data.input(&mut self.switcher).on_event(now_ms, data_plane::Input::Ext(ext));
            }
            SdnWorkerInput::Net(net) => {
                self.data.input(&mut self.switcher).on_event(now_ms, data_plane::Input::Net(net));
            }
            SdnWorkerInput::Bus(bus) => match bus {
                SdnWorkerBusEvent::Control(control) => {
                    let controller = self.controller.as_mut().expect("Should have controller");
                    controller.input(&mut self.switcher).on_event(now_ms, controller_plane::Input::Control(control));
                }
                SdnWorkerBusEvent::Workers(event) => {
                    self.data.input(&mut self.switcher).on_event(now_ms, data_plane::Input::Event(event));
                }
                SdnWorkerBusEvent::Worker(_, cross) => {
                    self.data.input(&mut self.switcher).on_event(now_ms, data_plane::Input::Worker(cross));
                }
            },
        }
    }

    pub fn on_shutdown(&mut self, now_ms: u64) {
        if self.shutdown {
            return;
        }
        log::info!("[SdnWorker] Shutdown");
        self.data.input(&mut self.switcher).on_shutdown(now_ms);
        if let Some(controller) = &mut self.controller {
            controller.input(&mut self.switcher).on_shutdown(now_ms);
        }
        self.shutdown = true;
    }

    pub fn pop_output2(&mut self, now: u64) -> Option<SdnWorkerOutput<UserData, SC, SE, TC, TW>> {
        loop {
            match self.switcher.current()?.try_into().ok()? {
                TaskType::Controller => {
                    if let Some(controller) = self.controller.as_mut() {
                        if let Some(out) = controller.pop_output(now, &mut self.switcher) {
                            return Some(self.process_controller_out(now, out));
                        }
                    } else {
                        self.switcher.finished(TaskType::Controller);
                    }
                }
                TaskType::Data => {
                    if let Some(out) = self.data.pop_output(now, &mut self.switcher) {
                        return Some(self.process_data_out(now, out));
                    }
                }
            }
        }
    }
}

impl<UserData, SC: Debug, SE: Debug, TC: Debug, TW: Debug> SdnWorker<UserData, SC, SE, TC, TW>
where
    UserData: 'static + Copy + Eq + Hash + Debug,
{
    fn process_controller_out(&mut self, now_ms: u64, out: controller_plane::Output<UserData, SE, TW>) -> SdnWorkerOutput<UserData, SC, SE, TC, TW> {
        match out {
            controller_plane::Output::Ext(out) => SdnWorkerOutput::Ext(out),
            controller_plane::Output::Event(event) => match event.dest() {
                LogicEventDest::Broadcast | LogicEventDest::Worker(_) => SdnWorkerOutput::Bus(SdnWorkerBusEvent::Workers(event)),
                LogicEventDest::Any => {
                    self.data.input(&mut self.switcher).on_event(now_ms, data_plane::Input::Event(event));
                    SdnWorkerOutput::Continue
                }
            },
            controller_plane::Output::OnResourceEmpty => {
                log::info!("[SdnWorker] controller plane OnResourceEmpty");
                SdnWorkerOutput::Continue
            }
        }
    }

    fn process_data_out(&mut self, now_ms: u64, out: data_plane::Output<UserData, SC, SE, TC>) -> SdnWorkerOutput<UserData, SC, SE, TC, TW> {
        match out {
            data_plane::Output::Ext(ext) => SdnWorkerOutput::ExtWorker(ext),
            data_plane::Output::Net(out) => SdnWorkerOutput::Net(out),
            data_plane::Output::Control(control) => {
                if let Some(controller) = &mut self.controller {
                    log::debug!("[SdnWorker] send control to controller {:?}", control);
                    controller.input(&mut self.switcher).on_event(now_ms, controller_plane::Input::Control(control));
                    SdnWorkerOutput::Continue
                } else {
                    SdnWorkerOutput::Bus(SdnWorkerBusEvent::Control(control))
                }
            }
            data_plane::Output::Worker(index, cross) => SdnWorkerOutput::Bus(SdnWorkerBusEvent::Worker(index, cross)),
            data_plane::Output::OnResourceEmpty => {
                log::info!("[SdnWorker] data plane OnResourceEmpty");
                SdnWorkerOutput::Continue
            }
            data_plane::Output::Continue => SdnWorkerOutput::Continue,
        }
    }
}

impl<UserData, SC: Debug, SE: Debug, TC: Debug, TW: Debug> TaskSwitcherChild<SdnWorkerOutput<UserData, SC, SE, TC, TW>> for SdnWorker<UserData, SC, SE, TC, TW>
where
    UserData: 'static + Copy + Eq + Hash + Debug,
{
    type Time = u64;

    fn empty_event(&self) -> SdnWorkerOutput<UserData, SC, SE, TC, TW> {
        SdnWorkerOutput::OnResourceEmpty
    }

    fn is_empty(&self) -> bool {
        self.shutdown && self.controller.as_ref().map_or(true, |c| c.is_empty()) && self.data.is_empty()
    }

    fn pop_output(&mut self, now: u64) -> Option<SdnWorkerOutput<UserData, SC, SE, TC, TW>> {
        self.pop_output2(now)
    }
}

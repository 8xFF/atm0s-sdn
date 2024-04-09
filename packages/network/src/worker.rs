use atm0s_sdn_identity::NodeId;
use sans_io_runtime::TaskSwitcher;

use crate::{
    controller_plane::{self, ControllerPlane, ControllerPlaneCfg},
    data_plane::{self, CrossWorker, DataPlane, DataPlaneCfg, NetInput, NetOutput},
    ExtIn, ExtOut, LogicControl, LogicEvent, LogicEventDest,
};

pub enum SdnWorkerBusEvent<SC, SE, TC, TW> {
    Control(LogicControl<SC, SE, TC>),
    Workers(LogicEvent<SE, TW>),
    Worker(u16, CrossWorker<SE>),
}

pub enum SdnWorkerInput<'a, SC, SE, TC, TW> {
    Ext(ExtIn<SC>),
    ExtWorker(ExtIn<SC>),
    Net(NetInput<'a>),
    Bus(SdnWorkerBusEvent<SC, SE, TC, TW>),
    ShutdownRequest,
}

pub enum SdnWorkerOutput<'a, SC, SE, TC, TW> {
    Ext(ExtOut<SE>),
    ExtWorker(ExtOut<SE>),
    Net(NetOutput<'a>),
    Bus(SdnWorkerBusEvent<SC, SE, TC, TW>),
    ShutdownResponse,
    Continue,
}

pub struct SdnWorkerCfg<SC, SE, TC, TW> {
    pub node_id: NodeId,
    pub controller: Option<ControllerPlaneCfg<SC, SE, TC, TW>>,
    pub data: DataPlaneCfg<SC, SE, TC, TW>,
}

pub struct SdnWorker<SC, SE, TC, TW> {
    controller: Option<ControllerPlane<SC, SE, TC, TW>>,
    data: DataPlane<SC, SE, TC, TW>,
    data_shutdown: bool,
    switcher: TaskSwitcher,
}

impl<SC, SE, TC, TW> SdnWorker<SC, SE, TC, TW> {
    pub fn new(cfg: SdnWorkerCfg<SC, SE, TC, TW>) -> Self {
        Self {
            controller: cfg.controller.map(|controller| ControllerPlane::new(cfg.node_id, controller)),
            data: DataPlane::new(cfg.node_id, cfg.data),
            data_shutdown: false,
            switcher: TaskSwitcher::new(2),
        }
    }

    pub fn on_tick<'a>(&mut self, now_ms: u64) -> Option<SdnWorkerOutput<'a, SC, SE, TC, TW>> {
        self.switcher.queue_flag_all();
        self.data.on_tick(now_ms);
        if let Some(controller) = &mut self.controller {
            controller.on_tick(now_ms);
            if let Some(out) = controller.pop_output(now_ms) {
                return Some(self.process_controller_out(now_ms, out));
            }
        }
        let out = self.data.pop_output(now_ms)?;
        Some(self.process_data_out(now_ms, out))
    }

    pub fn on_event<'a>(&mut self, now_ms: u64, input: SdnWorkerInput<'a, SC, SE, TC, TW>) -> Option<SdnWorkerOutput<'a, SC, SE, TC, TW>> {
        match input {
            SdnWorkerInput::Ext(ext) => {
                let controller = self.controller.as_mut().expect("Should have controller");
                controller.on_event(now_ms, controller_plane::Input::Ext(ext));
                let out = controller.pop_output(now_ms)?;
                Some(self.process_controller_out(now_ms, out))
            }
            SdnWorkerInput::ExtWorker(ext) => {
                let out = self.data.on_event(now_ms, data_plane::Input::Ext(ext))?;
                Some(self.process_data_out(now_ms, out))
            }
            SdnWorkerInput::Net(net) => {
                let out = self.data.on_event(now_ms, data_plane::Input::Net(net))?;
                Some(self.process_data_out(now_ms, out))
            }
            SdnWorkerInput::Bus(bus) => match bus {
                SdnWorkerBusEvent::Control(control) => {
                    let controller = self.controller.as_mut().expect("Should have controller");
                    controller.on_event(now_ms, controller_plane::Input::Control(control));
                    let out = controller.pop_output(now_ms)?;
                    Some(self.process_controller_out(now_ms, out))
                }
                SdnWorkerBusEvent::Workers(event) => {
                    let out = self.data.on_event(now_ms, data_plane::Input::Event(event))?;
                    Some(self.process_data_out(now_ms, out))
                }
                SdnWorkerBusEvent::Worker(_, cross) => {
                    let out = self.data.on_event(now_ms, data_plane::Input::Worker(cross))?;
                    Some(self.process_data_out(now_ms, out))
                }
            },
            SdnWorkerInput::ShutdownRequest => {
                self.switcher.queue_flag_all();
                if let Some(controller) = &mut self.controller {
                    controller.on_event(now_ms, controller_plane::Input::ShutdownRequest);
                }
                if let Some(out) = self.data.on_event(now_ms, data_plane::Input::ShutdownRequest) {
                    Some(self.process_data_out(now_ms, out))
                } else if let Some(controller) = &mut self.controller {
                    let out = controller.pop_output(now_ms)?;
                    Some(self.process_controller_out(now_ms, out))
                } else {
                    None
                }
            }
        }
    }

    pub fn pop_output<'a>(&mut self, now_ms: u64) -> Option<SdnWorkerOutput<'a, SC, SE, TC, TW>> {
        while let Some(current) = self.switcher.queue_current() {
            match current {
                0 => {
                    if let Some(controller) = &mut self.controller {
                        if let Some(out) = self.switcher.queue_process(controller.pop_output(now_ms)) {
                            return Some(self.process_controller_out(now_ms, out));
                        }
                    } else {
                        self.switcher.queue_process(None::<()>);
                    }
                }
                1 => {
                    if let Some(out) = self.switcher.queue_process(self.data.pop_output(now_ms)) {
                        return Some(self.process_data_out(now_ms, out));
                    }
                }
                _ => panic!("unknown task type"),
            }
        }
        None
    }
}

impl<SC, SE, TC, TW> SdnWorker<SC, SE, TC, TW> {
    fn process_controller_out<'a>(&mut self, now_ms: u64, out: controller_plane::Output<SE, TW>) -> SdnWorkerOutput<'a, SC, SE, TC, TW> {
        self.switcher.queue_flag_task(0);
        match out {
            controller_plane::Output::Ext(out) => SdnWorkerOutput::Ext(out),
            controller_plane::Output::Event(event) => match event.dest() {
                LogicEventDest::Broadcast | LogicEventDest::Worker(_) => SdnWorkerOutput::Bus(SdnWorkerBusEvent::Workers(event)),
                LogicEventDest::Any => {
                    if let Some(out) = self.data.on_event(now_ms, data_plane::Input::Event(event)) {
                        self.process_data_out(now_ms, out)
                    } else {
                        SdnWorkerOutput::Continue
                    }
                }
            },
            controller_plane::Output::ShutdownSuccess => {
                self.controller = None;
                SdnWorkerOutput::Continue
            }
        }
    }

    fn process_data_out<'a>(&mut self, now_ms: u64, out: data_plane::Output<'a, SC, SE, TC>) -> SdnWorkerOutput<'a, SC, SE, TC, TW> {
        self.switcher.queue_flag_task(1);
        match out {
            data_plane::Output::Ext(ext) => SdnWorkerOutput::ExtWorker(ext),
            data_plane::Output::Net(out) => SdnWorkerOutput::Net(out),
            data_plane::Output::Control(control) => {
                if let Some(controller) = &mut self.controller {
                    log::debug!("Send control to controller");
                    controller.on_event(now_ms, controller_plane::Input::Control(control));
                    if let Some(out) = controller.pop_output(now_ms) {
                        self.process_controller_out(now_ms, out)
                    } else {
                        SdnWorkerOutput::Continue
                    }
                } else {
                    SdnWorkerOutput::Bus(SdnWorkerBusEvent::Control(control))
                }
            }
            data_plane::Output::Worker(index, cross) => SdnWorkerOutput::Bus(SdnWorkerBusEvent::Worker(index, cross)),
            data_plane::Output::ShutdownResponse => {
                self.data_shutdown = true;
                SdnWorkerOutput::Continue
            }
            data_plane::Output::Continue => SdnWorkerOutput::Continue,
        }
    }
}
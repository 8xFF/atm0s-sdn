use std::{collections::VecDeque, net::SocketAddr, time::Instant};

use atm0s_sdn_identity::NodeAddr;
use atm0s_sdn_network::controller_plane::{ControllerPlane, Input as ControllerInput, Output as ControllerOutput};
use atm0s_sdn_router::shadow::ShadowRouterDelta;
use sans_io_runtime::{bus::BusEvent, Task, TaskInput, TaskOutput};

use crate::time::{TimePivot, TimeTicker};

pub type ChannelIn = ();
pub type ChannelOut = ();

pub struct ControllerPlaneCfg {
    pub node_id: u32,
    pub tick_ms: u64,
    #[cfg(feature = "vpn")]
    pub vpn_tun_device: sans_io_runtime::backend::tun::TunDevice,
}

#[derive(Debug, Clone)]
pub enum EventIn {
    ConnectTo(NodeAddr),
}

#[derive(Debug)]
pub enum EventOut {
    RouterRule(ShadowRouterDelta<SocketAddr>),
}

pub struct ControllerPlaneTask<TC, TW> {
    #[allow(unused)]
    node_id: u32,
    controller: ControllerPlane<TC, TW>,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
    ticker: TimeTicker,
    timer: TimePivot,
    #[cfg(feature = "vpn")]
    vpn_tun_device: sans_io_runtime::backend::tun::TunDevice,
}

impl<TC, TW> ControllerPlaneTask<TC, TW> {
    pub fn build(cfg: ControllerPlaneCfg) -> Self {
        Self {
            node_id: cfg.node_id,
            controller: ControllerPlane::new(cfg.node_id),
            queue: VecDeque::from([TaskOutput::Bus(BusEvent::ChannelSubscribe(()))]),
            ticker: TimeTicker::build(1000),
            timer: TimePivot::build(),
            #[cfg(feature = "vpn")]
            vpn_tun_device: cfg.vpn_tun_device,
        }
    }
}

impl<TC, TW> Task<ChannelIn, ChannelOut, EventIn, EventOut> for ControllerPlaneTask<TC, TW> {
    /// The type identifier for the task.
    const TYPE: u16 = 0;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        if self.ticker.tick(now) {
            self.controller.on_tick(self.timer.timestamp_ms(now));
        }
        self.pop_output(now)
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        let now_ms = self.timer.timestamp_ms(now);
        match input {
            TaskInput::Bus(_, event) => {
                todo!()
            }
            _ => {}
        };
        self.pop_output(now)
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        let now_ms = self.timer.timestamp_ms(now);
        if let Some(output) = self.queue.pop_front() {
            return Some(output);
        }
        let output = self.controller.pop_output(now_ms)?;
        match output {
            ControllerOutput::ShutdownSuccess => Some(TaskOutput::Destroy),
            _ => todo!(),
        }
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.controller.on_event(self.timer.timestamp_ms(now), ControllerInput::ShutdownRequest);
        self.pop_output(now)
    }
}

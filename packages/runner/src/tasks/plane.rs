use std::{net::SocketAddr, time::Instant};

use sans_io_runtime::{Task, TaskInput, TaskOutput};

use super::{
    connection::{self, ConnId},
    events::{ServiceId, TransportEvent},
};

pub type ChannelIn = ();
pub type ChannelOut = ();

pub struct PlaneCfg {}

#[derive(Debug, Clone)]
pub enum EventIn {
    Transport(TransportEvent),
    LocalNet(Vec<u8>),
    FromHandlerBus(ConnId, ServiceId, String),
}

pub enum EventOut {
    ToHandlerBus(ConnId, ServiceId, Vec<u8>),
    ConnectTo(SocketAddr),
    SpawnConnection(connection::SpawnCfg),
}

pub struct PlaneTask {}

impl PlaneTask {
    pub fn build(cfg: PlaneCfg) -> Self {
        Self {}
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for PlaneTask {
    /// The type identifier for the task.
    const TYPE: u16 = 0;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        todo!()
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        todo!()
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        todo!()
    }
}

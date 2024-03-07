use std::time::Instant;

use sans_io_runtime::{Task, TaskInput, TaskOutput};

use super::events::{BusEvent, ConnectionEvent, ServiceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnId(u64);

#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub rtt: u8,
}

pub type ChannelIn = ConnId;

pub enum ChannelOut {
    TransportWorker,
    Behaviour,
}

#[derive(Debug, Clone)]
pub enum EventIn {
    Net(ConnectionEvent),
    Bus(ServiceId, BusEvent<Vec<u8>>),
}

pub enum EventOut {
    Disconnected(ConnId),
    Net(ConnId, Vec<u8>),
    ToBehaviorBus(ConnId, ServiceId, String),
    ToHandleBus(ConnId, ConnId, ServiceId, Vec<u8>),
}

pub struct SpawnCfg {}

pub struct ConnectionTask {}

impl ConnectionTask {
    pub fn build(cfg: SpawnCfg) -> Self {
        Self {}
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for ConnectionTask {
    /// The type identifier for the task.
    const TYPE: u16 = 3;

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

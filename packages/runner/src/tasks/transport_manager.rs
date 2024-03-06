use std::{net::SocketAddr, time::Instant};

use sans_io_runtime::{Task, TaskInput, TaskOutput};

use super::{
    connection::{ConnId, ConnectionStats},
    events::{TransportEvent, TransportWorkerEvent},
};

pub type ChannelIn = ();
pub type ChannelOut = ();

#[derive(Debug, Clone)]
pub enum EventIn {
    ConnectTo(SocketAddr),
    UnhandleNetData(SocketAddr, Vec<u8>),
}

pub enum EventOut {
    Transport(TransportEvent),
    Worker(TransportWorkerEvent),
}

pub struct TransportManagerTask {}

impl TransportManagerTask {
    pub fn build() -> Self {
        Self {}
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for TransportManagerTask {
    const TYPE: u16 = 1;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        todo!()
    }

    fn on_input<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        todo!()
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        todo!()
    }
}

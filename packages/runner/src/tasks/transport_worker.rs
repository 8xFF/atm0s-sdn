use std::{net::SocketAddr, time::Instant};

use sans_io_runtime::{Task, TaskInput, TaskOutput};

use super::{
    connection::ConnId,
    events::{ConnectionEvent, TransportWorkerEvent},
};

pub type ChannelIn = ();
pub type ChannelOut = ();

pub type EventIn = TransportWorkerEvent;

pub enum EventOut {
    Connection(ConnId, ConnectionEvent),
    UnhandleData(SocketAddr, Vec<u8>),
}

pub struct TransportWorkerTask {}

impl TransportWorkerTask {
    pub fn build() -> Self {
        Self {}
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for TransportWorkerTask {
    /// The type identifier for the task.
    const TYPE: u16 = 2;

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

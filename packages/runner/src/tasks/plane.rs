use std::{collections::VecDeque, net::SocketAddr, time::Instant};

use sans_io_runtime::{bus::BusEvent, Task, TaskInput, TaskOutput};

use crate::tasks::connection::SpawnCfg;

use super::{
    connection::{self, ConnId},
    events::{ServiceId, TransportEvent},
};

pub type ChannelIn = ();
pub type ChannelOut = ();

pub struct PlaneCfg {
    pub node_id: u32,
}

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

pub struct PlaneTask {
    node_id: u32,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
}

impl PlaneTask {
    pub fn build(cfg: PlaneCfg) -> Self {
        Self {
            node_id: cfg.node_id,
            queue: VecDeque::from([TaskOutput::Bus(BusEvent::ChannelSubscribe(()))]),
        }
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for PlaneTask {
    /// The type identifier for the task.
    const TYPE: u16 = 0;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        let event = if let TaskInput::Bus(_, event) = input {
            event
        } else {
            panic!("Invalid input type for TransportManager")
        };

        match event {
            EventIn::Transport(event) => match event {
                TransportEvent::OutgoingError(_, _) => todo!(),
                TransportEvent::Connected(conn) => {
                    log::info!("New connection {:?} => will spawn new task", conn);
                    Some(TaskOutput::Bus(BusEvent::ChannelPublish(
                        (),
                        true,
                        EventOut::SpawnConnection(SpawnCfg { node_id: self.node_id, conn_id: conn }),
                    )))
                }
                TransportEvent::Disconnected(_) => todo!(),
            },
            EventIn::LocalNet(_) => todo!(),
            EventIn::FromHandlerBus(_, _, _) => todo!(),
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        None
    }

    fn shutdown<'a>(
            &mut self,
            now: Instant,
        ) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        None
    }
}

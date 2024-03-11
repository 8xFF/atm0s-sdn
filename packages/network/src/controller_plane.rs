use std::net::SocketAddr;

use atm0s_sdn_identity::{NodeAddr, NodeId};

use crate::event::DataEvent;

use self::connections::Connections;
pub use self::connections::ControlMsg;

mod connections;

pub enum Input {
    ConnectTo(NodeAddr),
    Data(SocketAddr, DataEvent),
    ShutdownRequest,
}

pub enum NetworkRule {}

pub enum Output {
    NetworkRule(NetworkRule),
    Data(SocketAddr, DataEvent),
    ShutdownSuccess,
}

pub struct ControllerPlane {
    conns: Connections,
}

impl ControllerPlane {
    pub fn new(node_id: NodeId) -> Self {
        Self { conns: Connections::new(node_id) }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.conns.on_tick(now_ms);
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input) {
        match event {
            Input::ConnectTo(addr) => {
                self.conns.on_event(now_ms, connections::Input::ConnectTo(addr));
            }
            Input::Data(remote, DataEvent::Control(msg)) => {
                self.conns.on_event(now_ms, connections::Input::ControlIn(remote, msg));
            }
            Input::Data(addr, DataEvent::Network(msg)) => {
                todo!()
            }
            Input::ShutdownRequest => {
                todo!()
            }
        }
    }

    pub fn pop_output(&mut self) -> Option<Output> {
        match self.conns.pop_output() {
            Some(connections::Output::ControlOut(remote, msg)) => Some(Output::Data(remote, DataEvent::Control(msg))),
            Some(connections::Output::ConnectionEvent(event)) => None,
            None => None,
        }
    }
}

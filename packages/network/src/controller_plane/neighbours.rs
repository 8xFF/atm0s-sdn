use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};

use crate::base::{ConnectionCtx, NeighboursControl};

mod connection;

pub enum NeighboursEvent {
    NeighbourConnected,
    NeighbourDisconnected,
}

pub enum Input {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    Control(SocketAddr, NeighboursControl),
}

pub enum Output {
    Control(SocketAddr, NeighboursControl),
    Event(NeighboursEvent),
}

pub struct NeighboursManager {
    node_id: NodeId,
}

impl NeighboursManager {
    pub fn new(node_id: NodeId) -> Self {
        Self { node_id }
    }

    pub fn conn<'a>(&self, conn: ConnId) -> Option<&'a ConnectionCtx> {
        None
    }

    pub fn on_tick(&mut self, now_ms: u64) {}

    pub fn on_input(&mut self, input: Input) {}

    pub fn pop_output(&mut self) -> Option<Output> {
        None
    }
}

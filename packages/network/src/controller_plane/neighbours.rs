use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};

use crate::base::NeighboursControl;

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

pub struct NeighboursManager {}

impl NeighboursManager {
    pub fn new() -> Self {
        todo!()
    }

    pub fn get_addr(&self, conn: ConnId) -> Option<SocketAddr> {
        None
    }

    pub fn on_tick(&mut self, now_ms: u64) {}

    pub fn on_input(&mut self, input: Input) {}

    pub fn pop_output(&mut self) -> Option<Output> {
        None
    }
}

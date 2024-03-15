use std::net::SocketAddr;

use atm0s_sdn_identity::NodeId;

mod connection;

pub enum NeighboursConnectError {
    AlreadyConnected,
    InvalidSignature,
    InvalidData,
}

pub enum NeighboursDisconnectReason {
    Shutdown,
    Other,
}

pub enum NeighboursDisconnectError {
    WrongSession,
    NotConnected,
    InvalidSignature,
}

pub enum NeighboursControl {
    ConnectRequest {
        from: NodeId,
        to: NodeId,
        session: u64,
        signature: Vec<u8>,
    },
    ConnectResponse {
        from: NodeId,
        to: NodeId,
        session: u64,
        signature: Vec<u8>,
        result: Result<(), NeighboursConnectError>,
    },
    Ping {
        session: u64,
        seq: u64,
        sent_us: u64,
    },
    Pong {
        session: u64,
        seq: u64,
        sent_us: u64,
    },
    DisconnectRequest {
        session: u64,
        signature: Vec<u8>,
        reason: NeighboursDisconnectReason,
    },
    DisconnectResponse {
        session: u64,
        signature: Vec<u8>,
        result: Result<(), NeighboursDisconnectError>,
    },
}

pub enum NeighboursEvent {
    NeighbourConnected,
    NeighbourDisconnected,
}

pub enum Output {
    Control(NeighboursControl),
    Event(NeighboursEvent),
}

pub struct NeighboursManager {}

impl NeighboursManager {
    pub fn new() -> Self {
        todo!()
    }

    pub fn on_tick(&mut self, now_ms: u64) {}

    pub fn on_remote(&mut self, remote: SocketAddr, msg: NeighboursControl) {}

    pub fn pop_output(&mut self) -> Option<Output> {
        None
    }
}

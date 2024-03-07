use std::{net::SocketAddr, time::Instant};

use serde::{Deserialize, Serialize};

use super::connection::{ConnId, ConnectionStats};

#[derive(Debug, Clone)]
pub struct ServiceId(u8);

#[derive(Debug, Clone)]
pub enum TransportEvent {
    OutgoingError(ConnId, String),
    Connected(ConnId),
    Disconnected(ConnId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionMessage {
    ConnectRequest { node_id: u32, meta: String, password: String },
    ConnectResponse(Result<u32, String>),
    Ping(u64, u32),
    Pong(u64, u32),
    Data(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Data(Instant, Vec<u8>),
    Stats(Instant, ConnectionStats),
}

#[derive(Debug, Clone)]
pub enum TransportWorkerEvent {
    PinConnection(ConnId, SocketAddr),
    UnPinConnection(ConnId),
    SendConn(ConnId, ConnectionMessage),
    SendTo(SocketAddr, ConnectionMessage),
}

#[derive(Debug, Clone)]
pub enum BusEvent<T> {
    FromBehavior(T),
    FromHandler(ConnId, T),
}

#[derive(Debug, Clone)]
pub enum BusAction {
    ToBehavior(String),
    ToHandler(ConnId, Vec<u8>),
}

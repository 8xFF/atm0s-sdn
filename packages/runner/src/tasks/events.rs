use std::{net::SocketAddr, time::Instant};

use super::connection::{ConnId, ConnectionStats};

#[derive(Debug, Clone)]
pub struct ServiceId(u8);

#[derive(Debug, Clone)]
pub enum TransportEvent {
    IncomingRequest(ConnId),
    IncomingConnection(ConnId),
    OutgoingConnection(ConnId),
    OutgoingError(ConnId),
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
    SendTo(ConnId, Vec<u8>),
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

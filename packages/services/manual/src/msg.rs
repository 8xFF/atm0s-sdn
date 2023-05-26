use bluesea_identity::PeerAddr;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Debug)]
pub enum ManualBehaviorEvent {}

#[derive(PartialEq, Debug)]
pub enum ManualHandlerEvent {}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum ManualMsg {}

#[derive(PartialEq, Debug)]
pub enum ManualReq {
    AddNeighbors(Vec<PeerAddr>),
    GetNeighbors(),
    GetConnections(),
}

#[derive(PartialEq, Debug)]
pub enum ConnectionState {
    OutgoingConnecting,
    OutgoingConnected,
    OutgoingError,
    IncomingConnected,
}

#[derive(PartialEq, Debug)]
pub enum ManualRes {
    AddNeighborsRes(usize),
    GetNeighborsRes(Vec<PeerAddr>),
    GetConnectionsRes(Vec<(u32, PeerAddr, ConnectionState)>),
}

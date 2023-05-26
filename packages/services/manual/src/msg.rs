use bluesea_identity::NodeAddr;
use serde::{Deserialize, Serialize};

#[derive(PartialEq, Debug)]
pub enum ManualBehaviorEvent {}

#[derive(PartialEq, Debug)]
pub enum ManualHandlerEvent {}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum ManualMsg {}

#[derive(PartialEq, Debug)]
pub enum ManualReq {
    AddNeighbors(Vec<NodeAddr>),
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
    GetNeighborsRes(Vec<NodeAddr>),
    GetConnectionsRes(Vec<(u32, NodeAddr, ConnectionState)>),
}

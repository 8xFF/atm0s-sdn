use atm0s_sdn_identity::{NodeAddr, NodeId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HandshakeRequest {
    pub node_id: NodeId,
    pub node_addr: NodeAddr,
    pub remote_node_id: NodeId,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum HandshakeResult {
    Success,
    AuthenticationError,
    Rejected,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum TcpMsg {
    ConnectRequest(HandshakeRequest, Vec<u8>),
    ConnectResponse(HandshakeResult, Vec<u8>),
    Ping(u64),
    Pong(u64),
    Msg(Vec<u8>),
}

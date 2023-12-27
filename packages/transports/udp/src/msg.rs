use atm0s_sdn_identity::{NodeAddr, NodeId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub node_id: NodeId,
    pub node_addr: NodeAddr,
    pub remote_node_id: NodeId,
    pub snow_handshake: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HandshakeResult {
    Success(Vec<u8>),
    AuthenticationError,
    Rejected,
}

#[derive(Serialize, Deserialize)]
pub enum UdpTransportMsg {
    ConnectRequest(HandshakeRequest, Vec<u8>),
    ConnectResponse(HandshakeResult, Vec<u8>),
    ConnectResponseAck(bool),
    Ping(u64),
    Pong(u64),
    Close,
}

pub fn build_control_msg<T: Serialize>(msg: &T) -> Vec<u8> {
    let res = bincode::serialize(msg).unwrap();
    let mut buf = vec![255];
    buf.extend(res);
    buf
}

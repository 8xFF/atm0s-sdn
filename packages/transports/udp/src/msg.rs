use bluesea_identity::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum HandshakeResult {
    Success,
    AuthenticationError,
    DestinationError,
    Rejected,
}

#[derive(Serialize, Deserialize)]
pub enum UdpTransportMsg {
    ConnectRequest(NodeId, String, NodeId),
    ConnectResponse(HandshakeResult),
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

use bluesea_identity::{NodeAddr, NodeId};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum TcpMsg {
    ConnectRequest(NodeId, NodeId, NodeAddr),
    ConnectResponse(Result<(NodeId, NodeAddr), String>),
    Ping(u64),
    Pong(u64),
    Msg(Vec<u8>),
}

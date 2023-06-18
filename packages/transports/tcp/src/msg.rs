use bluesea_identity::{NodeAddr, NodeId};
use network::transport::{ConnectionMsg, MsgRoute};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum TcpMsg<MSG> {
    ConnectRequest(NodeId, NodeId, NodeAddr),
    ConnectResponse(Result<(NodeId, NodeAddr), String>),
    Ping(u64),
    Pong(u64),
    Msg(MsgRoute, u8, u8, ConnectionMsg<MSG>),
}

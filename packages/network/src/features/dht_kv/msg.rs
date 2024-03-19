use atm0s_sdn_identity::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Hash, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Map(pub u64);

#[derive(Debug, Hash, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Key(pub u64);

#[derive(Debug, Hash, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version(pub u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Seq(pub u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct NodeSession(pub NodeId, pub u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DelReason {
    Timeout,
    Remote,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RemoteCommand {
    Client(NodeSession, ClientCommand),
    Server(NodeSession, ServerEvent),
}

// This part is for client related messages

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClientMapCommand {
    Set(Key, Version, Vec<u8>),
    Del(Key, Version),
    Sub(u64, Option<NodeSession>), //
    Unsub(u64),
    OnSetAck(Key, NodeSession, Version), //Seq from OnHSet
    OnDelAck(Key, NodeSession, Version), //Seq from OnHDel
}

impl ClientMapCommand {
    pub fn is_creator(&self) -> bool {
        match self {
            ClientMapCommand::Set(_, _, _) => true,
            ClientMapCommand::Sub(_, _) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ClientCommand {
    MapCmd(Map, ClientMapCommand),
    MapGet(Map, u64),
}

// This part is for server related messages

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ServerMapEvent {
    SetOk(Key, Version),
    DelOk(Key, Version),
    SubOk(u64),
    UnsubOk(u64),
    OnSet { key: Key, source: NodeSession, version: Version, data: Vec<u8> },
    OnDel { key: Key, source: NodeSession, version: Version },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum ServerEvent {
    MapEvent(Map, ServerMapEvent),
    MapGetRes(Map, u64, Vec<(Key, NodeSession, Version, Vec<u8>)>),
}

use atm0s_sdn_identity::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Hash, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Key(pub u64);

#[derive(Debug, Hash, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubKey(pub u64);

#[derive(Debug, Hash, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version(pub u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Seq(pub u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct NodeSession(pub NodeId, pub u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DelReason {
    Timeout,
    Remote,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum RemoteCommand {
    Client(NodeSession, ClientCommand),
    Server(NodeSession, ServerEvent),
}

// This part is for client related messages

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMapCommand {
    Set(SubKey, Version, Vec<u8>),
    Del(SubKey, Version),
    Sub(u64),
    Unsub(u64),
    OnSetAck(SubKey, Version), //Seq from OnHSet
    OnDelAck(SubKey, Version), //Seq from OnHDel
}

impl ClientMapCommand {
    pub fn is_creator(&self) -> bool {
        match self {
            ClientMapCommand::Set(_, _, _) => true,
            ClientMapCommand::Sub(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ClientCommand {
    MapCmd(Key, ClientMapCommand),
    MapGet(Key, u64),
}

// This part is for server related messages

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum ServerMapEvent {
    SetOk(SubKey, Version),
    DelOk(SubKey, Version),
    SubOk(u64),
    UnsubOk(u64),
    GetOk(u64, Vec<(SubKey, Version, NodeSession, Vec<u8>)>),
    OnSet { sub: SubKey, version: Version, source: NodeSession, data: Vec<u8> },
    OnDel { sub: SubKey, version: Version, source: NodeSession },
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum ServerEvent {
    Map(Key, ServerMapEvent),
}

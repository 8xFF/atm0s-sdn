use atm0s_sdn_identity::NodeId;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VirtualSocketControlMsg {
    ConnectRequest(String, HashMap<String, String>),
    ConnectReponse(bool),
    ConnectingPing,
    ConnectingPong,
    ConnectionClose(),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SocketId(pub NodeId, pub u32);
impl SocketId {
    pub fn node_id(&self) -> NodeId {
        self.0
    }

    pub fn client_id(&self) -> u32 {
        self.1
    }
}

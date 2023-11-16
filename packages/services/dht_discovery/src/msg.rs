use p_8xff_sdn_identity::{NodeAddr, NodeId};
use serde::{Deserialize, Serialize};

pub enum DiscoveryBehaviorEvent {
    OnNetworkMessage(DiscoveryMsg),
}

pub enum DiscoveryHandlerEvent {}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum DiscoveryMsg {
    FindKey(u32, NodeId),
    FindKeyRes(u32, Vec<(NodeId, NodeAddr)>),
}

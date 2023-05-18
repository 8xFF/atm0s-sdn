use bluesea_identity::{PeerAddr, PeerId};

pub enum DiscoveryBehaviorEvent {
    OnNetworkMessage(DiscoveryMsg)
}

pub enum DiscoveryHandlerEvent {

}

#[derive(PartialEq, Debug)]
pub enum DiscoveryMsg {
    FindKey(u32, PeerId),
    FindKeyRes(u32, Vec<(PeerId, PeerAddr)>),
}

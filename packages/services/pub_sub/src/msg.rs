use crate::relay::ChannelIdentify;
use bluesea_identity::NodeId;
use serde::{Deserialize, Serialize};

pub enum PubsubServiceBehaviourEvent {
    Awake,
    OnHashmapSet(u64, NodeId),
    OnHashmapDel(u64, NodeId),
}
pub enum PubsubServiceHandlerEvent {}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PubsubRemoteEvent {
    Sub(ChannelIdentify),
    Unsub(ChannelIdentify),
    SubAck(ChannelIdentify, bool),   //did it added, incase of false, it means it already subscribed
    UnsubAck(ChannelIdentify, bool), //did it removed, incase of false, it means it already unsubscribed
}

pub enum PubsubSdkEvent {
    Local(PubsubSdkMsg),
    FromNode(NodeId, PubsubSdkMsg),
}

pub enum PubsubSdkMsg {
    Sub(ChannelIdentify),
    Unsub(ChannelIdentify),
    SubAck(ChannelIdentify, bool),
    UnsubAck(ChannelIdentify, bool),
}

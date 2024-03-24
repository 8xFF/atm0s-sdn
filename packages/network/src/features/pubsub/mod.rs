use std::net::SocketAddr;

use atm0s_sdn_identity::NodeId;

use crate::base::FeatureControlActor;

use self::msg::{ChannelId, RelayControl, RelayId};

mod controller;
mod msg;
mod worker;

pub use controller::PubSubFeature;
pub use worker::PubSubFeatureWorker;

pub const FEATURE_ID: u8 = 5;
pub const FEATURE_NAME: &str = "pubsub";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelControl {
    SubSource(NodeId),
    UnsubSource(NodeId),
    PubData(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control(ChannelId, ChannelControl);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelEvent {
    SourceData(NodeId, Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event(ChannelId, ChannelEvent);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayWorkerControl {
    SendSub(u64, Option<SocketAddr>),
    SendUnsub(u64, SocketAddr),
    SendSubOk(u64, SocketAddr),
    SendUnsubOk(u64, SocketAddr),
    RouteSet(SocketAddr),
    RouteDel(SocketAddr),
    RouteSetLocal(FeatureControlActor),
    RouteDelLocal(FeatureControlActor),
    RouteSetRemote(SocketAddr),
    RouteDelRemote(SocketAddr),
}

impl RelayWorkerControl {
    pub fn is_broadcast(&self) -> bool {
        match self {
            RelayWorkerControl::SendSub(_, _) => false,
            RelayWorkerControl::SendUnsub(_, _) => false,
            RelayWorkerControl::SendSubOk(_, _) => false,
            RelayWorkerControl::SendUnsubOk(_, _) => false,
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ToWorker {
    RelayWorkerControl(RelayId, RelayWorkerControl),
    RelayData(RelayId, Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum ToController {
    RemoteControl(SocketAddr, RelayId, RelayControl),
}

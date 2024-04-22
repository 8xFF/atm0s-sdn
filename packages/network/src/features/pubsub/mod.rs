use std::net::SocketAddr;

use atm0s_sdn_identity::NodeId;

use crate::base::FeatureControlActor;

use self::msg::{RelayControl, RelayId, SourceHint};

mod controller;
mod msg;
mod worker;

pub use controller::PubSubFeature;
pub use msg::{ChannelId, Feedback};
pub use worker::PubSubFeatureWorker;

pub const FEATURE_ID: u8 = 5;
pub const FEATURE_NAME: &str = "pubsub";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelControl {
    SubAuto,
    FeedbackAuto(Feedback),
    UnsubAuto,
    SubSource(NodeId),
    UnsubSource(NodeId),
    PubStart,
    PubData(Vec<u8>),
    PubStop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control(pub ChannelId, pub ChannelControl);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelEvent {
    RouteChanged(NodeId),
    SourceData(NodeId, Vec<u8>),
    FeedbackData(Feedback),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event(pub ChannelId, pub ChannelEvent);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayWorkerControl<UserData> {
    SendSub(u64, Option<SocketAddr>),
    SendUnsub(u64, SocketAddr),
    SendSubOk(u64, SocketAddr),
    SendUnsubOk(u64, SocketAddr),
    SendRouteChanged,
    SendFeedback(Feedback, SocketAddr),
    RouteSetSource(SocketAddr),
    RouteDelSource(SocketAddr),
    RouteSetLocal(FeatureControlActor<UserData>),
    RouteDelLocal(FeatureControlActor<UserData>),
    RouteSetRemote(SocketAddr, u64),
    RouteDelRemote(SocketAddr),
}

impl<UserData> RelayWorkerControl<UserData> {
    pub fn is_broadcast(&self) -> bool {
        match self {
            RelayWorkerControl::SendSub(_, _) => false,
            RelayWorkerControl::SendUnsub(_, _) => false,
            RelayWorkerControl::SendSubOk(_, _) => false,
            RelayWorkerControl::SendUnsubOk(_, _) => false,
            RelayWorkerControl::SendRouteChanged => false,
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ToWorker<UserData> {
    RelayControl(RelayId, RelayWorkerControl<UserData>),
    SourceHint(ChannelId, Option<SocketAddr>, SourceHint),
    RelayData(RelayId, Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum ToController {
    RelayControl(SocketAddr, RelayId, RelayControl),
    SourceHint(SocketAddr, ChannelId, SourceHint),
}

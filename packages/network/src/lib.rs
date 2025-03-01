#![allow(clippy::bool_assert_comparison)]

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::RouteRule;
use base::{FeatureControlActor, NeighboursControl, NetIncomingMeta, NetOutgoingMeta, SecureContext, ServiceControlActor, ServiceId};
use data_plane::{CrossWorker, NetPair};
use features::{Features, FeaturesControl, FeaturesEvent, FeaturesToController, FeaturesToWorker};
use sans_io_runtime::Buffer;

#[cfg(feature = "fuzz")]
pub mod _fuzz_export;
pub mod base;
pub mod controller_plane;
pub mod data_plane;
pub mod features;
pub mod secure;
pub mod services;
pub mod worker;

#[derive(Debug, Clone)]
pub enum ExtIn<UserData, ServicesControl> {
    FeaturesControl(UserData, FeaturesControl),
    ServicesControl(ServiceId, UserData, ServicesControl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtOut<UserData, ServicesEvent> {
    FeaturesEvent(UserData, FeaturesEvent),
    ServicesEvent(ServiceId, UserData, ServicesEvent),
}

#[derive(Debug, Clone)]
pub enum LogicControl<UserData, SC, SE, TC> {
    Feature(FeaturesToController),
    Service(ServiceId, TC),
    NetNeighbour(NetPair, NeighboursControl),
    NetRemote(Features, ConnId, NetIncomingMeta, Buffer),
    NetLocal(Features, NetIncomingMeta, Buffer),
    FeaturesControl(FeatureControlActor<UserData>, FeaturesControl),
    ServicesControl(ServiceControlActor<UserData>, ServiceId, SC),
    ServiceEvent(ServiceId, FeaturesEvent),
    ExtFeaturesEvent(UserData, FeaturesEvent),
    ExtServicesEvent(ServiceId, UserData, SE),
}

#[derive(Debug, Clone)]
pub enum LogicEvent<UserData, SE, TW> {
    NetNeighbour(NetPair, NeighboursControl),
    NetDirect(Features, NetPair, ConnId, NetOutgoingMeta, Buffer),
    NetRoute(Features, RouteRule, NetOutgoingMeta, Buffer),

    Pin(ConnId, NodeId, NetPair, SecureContext),
    UnPin(ConnId),
    /// first bool is flag for broadcast or not
    Feature(bool, FeaturesToWorker<UserData>),
    Service(ServiceId, TW),
    /// first u16 is worker id
    ExtFeaturesEvent(u16, UserData, FeaturesEvent),
    /// first u16 is worker id
    ExtServicesEvent(u16, ServiceId, UserData, SE),
}

pub enum LogicEventDest<UserData, SE, TW> {
    Broadcast(LogicEvent<UserData, SE, TW>),
    Worker(u16, CrossWorker<UserData, SE>),
    Any(LogicEvent<UserData, SE, TW>),
}

impl<UserData, SE, TW> LogicEvent<UserData, SE, TW> {
    pub fn with_dest(self) -> LogicEventDest<UserData, SE, TW> {
        match self {
            LogicEvent::Pin(..) => LogicEventDest::Broadcast(self),
            LogicEvent::UnPin(..) => LogicEventDest::Broadcast(self),
            LogicEvent::Service(..) => LogicEventDest::Broadcast(self),
            LogicEvent::Feature(true, ..) => LogicEventDest::Broadcast(self),
            LogicEvent::Feature(false, ..) => LogicEventDest::Any(self),
            LogicEvent::NetNeighbour(_, _) => LogicEventDest::Any(self),
            LogicEvent::NetDirect(_, _, _, _, _) => LogicEventDest::Any(self),
            LogicEvent::NetRoute(_, _, _, _) => LogicEventDest::Any(self),
            LogicEvent::ExtFeaturesEvent(worker, user_data, event) => LogicEventDest::Worker(worker, CrossWorker::Feature(user_data, event)),
            LogicEvent::ExtServicesEvent(worker, service_id, user_data, event) => LogicEventDest::Worker(worker, CrossWorker::Service(service_id, user_data, event)),
        }
    }
}

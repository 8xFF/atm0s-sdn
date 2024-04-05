use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::RouteRule;
use base::{FeatureControlActor, NeighboursControl, NetIncomingMeta, NetOutgoingMeta, SecureContext, ServiceControlActor, ServiceId};
pub use convert_enum;
use features::{Features, FeaturesControl, FeaturesEvent, FeaturesToController, FeaturesToWorker};

#[cfg(feature = "fuzz")]
pub mod _fuzz_export;
pub mod base;
pub mod controller_plane;
pub mod data_plane;
pub mod features;
pub mod secure;
pub mod services;

#[derive(Debug, Clone, convert_enum::From)]
pub enum ExtIn<ServicesControl> {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    FeaturesControl(FeaturesControl),
    #[convert_enum(optout)]
    ServicesControl(ServiceId, ServicesControl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtOut<ServicesEvent> {
    FeaturesEvent(FeaturesEvent),
    ServicesEvent(ServiceId, ServicesEvent),
}

#[derive(Debug, Clone)]
pub enum LogicControl<SC, SE, TC> {
    Feature(FeaturesToController),
    Service(ServiceId, TC),
    NetNeighbour(SocketAddr, NeighboursControl),
    NetRemote(Features, ConnId, NetIncomingMeta, Vec<u8>),
    NetLocal(Features, NetIncomingMeta, Vec<u8>),
    FeaturesControl(FeatureControlActor, FeaturesControl),
    ServicesControl(ServiceControlActor, ServiceId, SC),
    ServiceEvent(ServiceId, FeaturesEvent),
    ExtFeaturesEvent(FeaturesEvent),
    ExtServicesEvent(ServiceId, SE),
}

#[derive(Debug, Clone)]
pub enum LogicEvent<SE, TW> {
    NetNeighbour(SocketAddr, NeighboursControl),
    NetDirect(Features, SocketAddr, ConnId, NetOutgoingMeta, Vec<u8>),
    NetRoute(Features, RouteRule, NetOutgoingMeta, Vec<u8>),

    Pin(ConnId, NodeId, SocketAddr, SecureContext),
    UnPin(ConnId),
    /// first bool is flag for broadcast or not
    Feature(bool, FeaturesToWorker),
    Service(ServiceId, TW),
    /// first u16 is worker id
    ExtFeaturesEvent(u16, FeaturesEvent),
    /// first u16 is worker id
    ExtServicesEvent(u16, ServiceId, SE),
}

pub enum LogicEventDest {
    Broadcast,
    Any,
    Worker(u16),
}

impl<SE, TW> LogicEvent<SE, TW> {
    pub fn dest(&self) -> LogicEventDest {
        match self {
            LogicEvent::Pin(..) => LogicEventDest::Broadcast,
            LogicEvent::UnPin(..) => LogicEventDest::Broadcast,
            LogicEvent::Service(..) => LogicEventDest::Broadcast,
            LogicEvent::Feature(true, ..) => LogicEventDest::Broadcast,
            LogicEvent::Feature(false, ..) => LogicEventDest::Any,
            LogicEvent::NetNeighbour(_, _) => LogicEventDest::Any,
            LogicEvent::NetDirect(_, _, _, _, _) => LogicEventDest::Any,
            LogicEvent::NetRoute(_, _, _, _) => LogicEventDest::Any,
            LogicEvent::ExtFeaturesEvent(worker, _) => LogicEventDest::Worker(*worker),
            LogicEvent::ExtServicesEvent(worker, _, _) => LogicEventDest::Worker(*worker),
        }
    }
}

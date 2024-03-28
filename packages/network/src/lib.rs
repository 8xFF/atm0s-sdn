use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::RouteRule;
use base::{FeatureControlActor, NeighboursControl, NetIncomingMeta, NetOutgoingMeta, SecureContext, ServiceId};
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

pub mod san_io_utils;

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
    ServicesEvent(ServicesEvent),
}

#[derive(Debug, Clone)]
pub enum LogicControl<TC> {
    Feature(FeaturesToController),
    Service(ServiceId, TC),
    NetNeighbour(SocketAddr, NeighboursControl),
    NetRemote(Features, ConnId, NetIncomingMeta, Vec<u8>),
    NetLocal(Features, NetIncomingMeta, Vec<u8>),
    FeaturesControl(FeatureControlActor, FeaturesControl),
    ServiceEvent(ServiceId, FeaturesEvent),
}

#[derive(Debug, Clone)]
pub enum LogicEvent<TW> {
    NetNeighbour(SocketAddr, NeighboursControl),
    NetDirect(Features, SocketAddr, ConnId, NetOutgoingMeta, Vec<u8>),
    NetRoute(Features, RouteRule, NetOutgoingMeta, Vec<u8>),

    Pin(ConnId, NodeId, SocketAddr, SecureContext),
    UnPin(ConnId),
    /// first bool is flag for broadcast or not
    Feature(bool, FeaturesToWorker),
    Service(ServiceId, TW),
}

impl<TW> LogicEvent<TW> {
    pub fn is_broadcast(&self) -> bool {
        match self {
            LogicEvent::Pin(..) => true,
            LogicEvent::UnPin(..) => true,
            LogicEvent::Feature(is_broadcast, ..) => *is_broadcast,
            LogicEvent::Service(..) => true,
            _ => false,
        }
    }
}

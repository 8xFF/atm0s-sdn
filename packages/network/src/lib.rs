use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::RouteRule;
use base::{FeatureControlActor, NeighboursControl, SecureContext, ServiceId};
pub use convert_enum;
use features::{Features, FeaturesControl, FeaturesEvent, FeaturesToController, FeaturesToWorker};

pub mod base;
pub mod controller_plane;
pub mod data_plane;
pub mod features;
pub mod services;

pub mod san_io_utils;

#[derive(Debug, Clone)]
pub enum ExtIn {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    FeaturesControl(FeaturesControl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtOut {
    FeaturesEvent(FeaturesEvent),
}

#[derive(Debug, Clone)]
pub enum LogicControl<TC> {
    Feature(FeaturesToController),
    Service(ServiceId, TC),
    NetNeighbour(SocketAddr, NeighboursControl),
    NetRemote(Features, ConnId, Vec<u8>),
    NetLocal(Features, Vec<u8>),
    FeaturesControl(FeatureControlActor, FeaturesControl),
    ServiceEvent(ServiceId, FeaturesEvent),
}

#[derive(Debug, Clone)]
pub enum LogicEvent<TW> {
    NetNeigbour(SocketAddr, NeighboursControl),
    NetDirect(Features, ConnId, Vec<u8>),
    NetRoute(Features, RouteRule, Vec<u8>),

    Pin(ConnId, SocketAddr, SecureContext),
    UnPin(ConnId),
    Feature(FeaturesToWorker),
    Service(ServiceId, TW),
}

impl<TW> LogicEvent<TW> {
    pub fn is_broadcast(&self) -> bool {
        match self {
            LogicEvent::Pin(..) => true,
            LogicEvent::UnPin(..) => true,
            LogicEvent::Feature(..) => true,
            LogicEvent::Service(..) => true,
            _ => false,
        }
    }
}

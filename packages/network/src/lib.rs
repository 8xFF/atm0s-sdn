use atm0s_sdn_identity::{NodeAddr, NodeId};
pub use convert_enum;
use features::{FeaturesControl, FeaturesEvent};

pub mod base;
pub mod controller_plane;
pub mod convert;
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

#[derive(Debug, Clone)]
pub enum ExtOut {
    FeaturesEvent(FeaturesEvent),
}

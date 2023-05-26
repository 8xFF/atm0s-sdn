use crate::table::Path;
use bluesea_identity::{ConnId, NodeId};

mod registry;
mod router;
mod table;
mod utils;

#[derive(PartialEq, Debug)]
pub enum ServiceDestination {
    Local,
    Remote(ConnId, NodeId),
}

#[derive(PartialEq, Debug)]
pub enum NodeDestination {
    Local,
    Remote(ConnId, NodeId),
}

#[derive(PartialEq, Debug)]
pub enum NodeDestinationPath {
    Local,
    Remote(Path),
}

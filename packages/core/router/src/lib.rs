use bluesea_identity::{ConnId, NodeId};

mod registry;
mod router;
mod shared;
mod table;
mod utils;

pub use crate::router::RouterSync;
pub use crate::shared::SharedRouter;
pub use crate::table::{Metric, Path};

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

use bluesea_identity::{ConnId, NodeId};

mod registry;
mod router;
mod shared;
mod table;
mod utils;

pub use crate::registry::{Registry, RegistrySync};
pub use crate::router::{Router, RouterSync};
pub use crate::shared::SharedRouter;
pub use crate::table::{Metric, Path};

#[derive(PartialEq, Debug)]
pub enum ServiceDestination {
    Local,
    Remote(ConnId, NodeId),
}

use atm0s_sdn_identity::{ConnId, NodeId};

mod registry;
mod router;
mod table;

pub use self::registry::{Registry, RegistryDelta, RegistryDestDelta, RegistrySync};
pub use self::router::{Router, RouterDelta, RouterSync};
pub use self::table::{DestDelta, Metric, Path, TableDelta};

#[derive(PartialEq, Debug)]
pub enum ServiceDestination {
    Local,
    Remote(ConnId, NodeId),
}

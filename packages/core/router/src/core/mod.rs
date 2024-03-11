use atm0s_sdn_identity::{ConnId, NodeId};

mod registry;
mod router;
mod table;

pub use self::registry::{Registry, RegistrySync};
pub use self::router::{Router, RouterSync};
pub use self::table::{Metric, Path};

#[derive(PartialEq, Debug)]
pub enum ServiceDestination {
    Local,
    Remote(ConnId, NodeId),
}

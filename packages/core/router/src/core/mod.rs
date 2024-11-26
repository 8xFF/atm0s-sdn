use atm0s_sdn_identity::{ConnId, NodeId};

mod registry;
mod router;
mod table;

pub use self::registry::{RegisterDestDump, RegisterDump, Registry, RegistryDelta, RegistryDestDelta, RegistrySync};
pub use self::router::{Router, RouterDelta, RouterDump, RouterSync};
pub use self::table::{DestDelta, DestDump, Metric, Path, TableDelta, TableDump, TableSync, BANDWIDTH_LIMIT};

#[derive(PartialEq, Debug)]
pub enum ServiceDestination {
    Local,
    Remote(ConnId, NodeId),
}

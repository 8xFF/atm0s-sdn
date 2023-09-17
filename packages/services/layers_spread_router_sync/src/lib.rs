pub static FAST_PATH_ROUTE_SERVICE_ID: u8 = 3;

mod behavior;
mod handler;
mod mgs;

pub use behavior::LayersSpreadRouterSyncBehavior;
pub use mgs::*;

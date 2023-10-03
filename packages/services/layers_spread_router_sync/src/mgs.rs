use layers_spread_router::RouterSync;
use serde::{Deserialize, Serialize};

pub enum LayersSpreadRouterSyncBehaviorEvent {}

pub enum LayersSpreadRouterSyncHandlerEvent {}

#[derive(Serialize, Deserialize)]
pub enum LayersSpreadRouterSyncMsg {
    Sync(RouterSync),
}

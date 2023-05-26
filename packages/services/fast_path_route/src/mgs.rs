use router::RouterSync;
use serde::{Deserialize, Serialize};

pub enum FastPathRouteBehaviorEvent {}

pub enum FastPathRouteHandlerEvent {}

#[derive(Serialize, Deserialize)]
pub enum FastPathRouteMsg {
    Sync(RouterSync),
}

pub enum FastPathRouteReq {}

pub enum FastPathRouteRes {}

use atm0s_sdn_identity::ConnId;
use atm0s_sdn_router::core::RouterDelta;
use serde::{Deserialize, Serialize};

use crate::msg::TransportMsg;

use super::connections::{ConnectionCtx, ConnectionStats};

#[repr(u8)]
pub enum FeatureType {
    UserCustomFeature = 0,
    DhtKeyValue = 1,
    LazyKeyValue = 2,
    Pubsub = 3,
}

pub enum FeatureOutput {
    RouterRule(RouterDelta),
    NetData(ConnId, TransportMsg),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeatureMsg {}

pub trait Feature: Send + Sync {
    fn feature_type(&self) -> u8;
    fn feature_name(&self) -> &str;
    fn on_tick(&mut self, _now: u64) {}
    fn on_conn_connected(&mut self, _now: u64, _ctx: &ConnectionCtx) {}
    fn on_conn_data(&mut self, _now: u64, _ctx: &ConnectionCtx, _msg: TransportMsg) {}
    fn on_conn_stats(&mut self, _now: u64, _ctx: &ConnectionCtx, _stats: &ConnectionStats) {}
    fn on_conn_disconnected(&mut self, _now: u64, _ctx: &ConnectionCtx) {}
    fn pop_output(&mut self) -> Option<FeatureOutput> {
        None
    }
}

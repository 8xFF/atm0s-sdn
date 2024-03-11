use atm0s_sdn_identity::ConnId;

use crate::msg::TransportMsg;

use super::{
    connections::{ConnectionCtx, ConnectionStats},
    NetworkRule,
};

pub enum ServiceOutput {
    NetworkRule(NetworkRule),
    NetData(ConnId, TransportMsg),
}

pub trait Service: Send + Sync {
    fn service_type(&self) -> u8;
    fn service_name(&self) -> &str;
    fn on_tick(&mut self, _now: u64) {}
    fn on_conn_connected(&mut self, _now: u64, _ctx: &ConnectionCtx) {}
    fn on_conn_data(&mut self, _now: u64, _ctx: &ConnectionCtx, _msg: TransportMsg) {}
    fn on_conn_stats(&mut self, _now: u64, _ctx: &ConnectionCtx, _stats: &ConnectionStats) {}
    fn on_conn_disconnected(&mut self, _now: u64, _ctx: &ConnectionCtx) {}
    fn pop_output(&mut self) -> Option<ServiceOutput> {
        None
    }
}

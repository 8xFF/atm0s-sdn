pub const SERVICE_TYPE: u8 = 0;
pub const SERVICE_NAME: &str = "router_sync";

use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{
    core::{Metric, Router, RouterSync},
    RouteRule,
};

use crate::{
    controller_plane::{
        connections::{ConnectionCtx, ConnectionStats},
        Service, ServiceOutput,
    },
    msg::TransportMsg,
};

pub struct RouterSyncService {
    node_id: NodeId,
    router: Router,
    metric: Option<Metric>,
    conns: HashMap<ConnId, ConnectionCtx>,
    queue: VecDeque<ServiceOutput>,
}

impl RouterSyncService {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            router: Router::new(node_id),
            metric: None,
            conns: HashMap::new(),
            queue: VecDeque::new(),
        }
    }
}

impl Service for RouterSyncService {
    fn service_type(&self) -> u8 {
        SERVICE_TYPE
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn on_tick(&mut self, _now: u64) {
        for (id, ctx) in &self.conns {
            let sync = self.router.create_sync(ctx.node);
            let sync_msg = TransportMsg::build(SERVICE_TYPE, SERVICE_TYPE, RouteRule::Direct, 0, 0, &bincode::serialize(&sync).expect("Should create sync"));
            self.queue.push_back(ServiceOutput::NetData(*id, sync_msg));
        }
    }

    fn on_conn_connected(&mut self, _now: u64, ctx: &ConnectionCtx) {
        log::info!("RouterSyncService::on_conn_connected {}, remote {}", ctx.node, ctx.remote);
        self.conns.insert(ctx.id, ctx.clone());
    }

    fn on_conn_data(&mut self, _now: u64, ctx: &ConnectionCtx, msg: TransportMsg) {
        if let Ok(sync_msg) = msg.get_payload_bincode::<RouterSync>() {
            if let Some(metric) = &self.metric {
                log::debug!("RouterSyncService::on_conn_data sync router from {}, remote {}", ctx.node, ctx.remote);
                self.router.apply_sync(ctx.id, ctx.node, metric.clone(), sync_msg);
            } else {
                log::warn!("RouterSyncService::on_conn_data sync router from {}, remote {} failed, no metric available", ctx.node, ctx.remote);
            }
        }
    }

    fn on_conn_stats(&mut self, _now: u64, ctx: &ConnectionCtx, stats: &ConnectionStats) {
        log::debug!("RouterSyncService::on_conn_stats {}, remote {}, rtt {}", ctx.node, ctx.remote, stats.rtt);
        let metric = Metric::new(stats.rtt, vec![ctx.node, self.node_id], 10000000);
        self.metric = Some(metric.clone());
        self.router.set_direct(ctx.id, ctx.node, metric);
    }

    fn on_conn_disconnected(&mut self, _now: u64, ctx: &ConnectionCtx) {
        log::info!("RouterSyncService::on_conn_disconnected {}, remote {}", ctx.node, ctx.remote);
        self.router.del_direct(ctx.id);
        self.conns.remove(&ctx.id);
    }

    fn pop_output(&mut self) -> Option<ServiceOutput> {
        self.queue.pop_front()
    }
}

use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::core::{Metric, Router, RouterDelta, RouterSync};

use crate::controller_plane::connections::{ConnectionCtx, ConnectionStats};

pub enum Output {
    RouterRule(RouterDelta),
    NetData(ConnId, RouterSync),
}

pub struct RouterSyncLogic {
    node_id: NodeId,
    router: Router,
    metric: Option<Metric>,
    conns: HashMap<ConnId, ConnectionCtx>,
    queue: VecDeque<Output>,
}

impl RouterSyncLogic {
    pub fn new(node_id: NodeId, services_id: Vec<u8>) -> Self {
        let mut router = Router::new(node_id);
        for service_id in services_id {
            router.register_service(service_id);
        }
        Self {
            node_id,
            router,
            metric: None,
            conns: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, _now: u64) {
        for (id, ctx) in &self.conns {
            let sync = self.router.create_sync(ctx.node);
            self.queue.push_back(Output::NetData(*id, sync));
        }

        self.router.log_dump();
    }

    pub fn on_conn_connected(&mut self, _now: u64, ctx: &ConnectionCtx) {
        log::info!("RouterSyncService::on_conn_connected {}, remote {}", ctx.node, ctx.remote);
        self.conns.insert(ctx.id, ctx.clone());
    }

    pub fn on_conn_router_sync(&mut self, _now: u64, ctx: &ConnectionCtx, sync_msg: RouterSync) {
        if let Some(metric) = &self.metric {
            log::debug!("RouterSyncService::on_conn_data sync router from {}, remote {}", ctx.node, ctx.remote);
            self.router.apply_sync(ctx.id, ctx.node, metric.clone(), sync_msg);
        } else {
            log::warn!("RouterSyncService::on_conn_data sync router from {}, remote {} failed, no metric available", ctx.node, ctx.remote);
        }
    }

    pub fn on_conn_stats(&mut self, _now: u64, ctx: &ConnectionCtx, stats: &ConnectionStats) {
        log::debug!("RouterSyncService::on_conn_stats {}, remote {}, rtt {}", ctx.node, ctx.remote, stats.rtt);
        let metric = Metric::new(stats.rtt, vec![ctx.node, self.node_id], 10000000);
        self.metric = Some(metric.clone());
        self.router.set_direct(ctx.id, ctx.node, metric);
    }

    pub fn on_conn_disconnected(&mut self, _now: u64, ctx: &ConnectionCtx) {
        log::info!("RouterSyncService::on_conn_disconnected {}, remote {}", ctx.node, ctx.remote);
        self.router.del_direct(ctx.id);
        self.conns.remove(&ctx.id);
    }

    pub fn pop_output(&mut self) -> Option<Output> {
        while let Some(delta) = self.router.pop_delta() {
            self.queue.push_back(Output::RouterRule(delta));
        }
        self.queue.pop_front()
    }
}

use std::collections::VecDeque;

use crate::mgs::{LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, LayersSpreadRouterSyncMsg};
use crate::FAST_PATH_ROUTE_SERVICE_ID;
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::RouteRule;
use layers_spread_router::{Metric, RouterSync, SharedRouter};
use network::behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction};
use network::msg::TransportMsg;
use network::transport::ConnectionEvent;

pub struct LayersSpreadRouterSyncHandler<BE, HE> {
    router: SharedRouter,
    wait_sync: Option<RouterSync>,
    metric: Option<Metric>,
    actions: VecDeque<ConnectionHandlerAction<BE, HE>>,
}

impl<BE, HE> LayersSpreadRouterSyncHandler<BE, HE>
where
    BE: From<LayersSpreadRouterSyncBehaviorEvent> + TryInto<LayersSpreadRouterSyncBehaviorEvent> + Send + Sync + 'static,
    HE: From<LayersSpreadRouterSyncHandlerEvent> + TryInto<LayersSpreadRouterSyncHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(router: SharedRouter) -> Self {
        Self {
            router,
            wait_sync: None,
            metric: None,
            actions: VecDeque::new(),
        }
    }

    fn send_sync(&mut self, ctx: &ConnectionContext) {
        let sync = self.router.create_sync(ctx.remote_node_id);
        log::info!("[LayersSpreadRouterHander {} {}/{}] send RouterSync", ctx.local_node_id, ctx.remote_node_id, ctx.conn_id);
        let sync_msg = TransportMsg::build_reliable(FAST_PATH_ROUTE_SERVICE_ID, RouteRule::Direct, 0, &bincode::serialize(&LayersSpreadRouterSyncMsg::Sync(sync)).unwrap());
        self.actions.push_back(ConnectionHandlerAction::ToNet(sync_msg));
    }
}

impl<BE, HE> ConnectionHandler<BE, HE> for LayersSpreadRouterSyncHandler<BE, HE>
where
    BE: From<LayersSpreadRouterSyncBehaviorEvent> + TryInto<LayersSpreadRouterSyncBehaviorEvent> + Send + Sync + 'static,
    HE: From<LayersSpreadRouterSyncHandlerEvent> + TryInto<LayersSpreadRouterSyncHandlerEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, ctx: &ConnectionContext, _now_ms: u64) {
        self.send_sync(ctx);
    }

    fn on_tick(&mut self, ctx: &ConnectionContext, _now_ms: u64, _interal_ms: u64) {
        self.send_sync(ctx);
    }

    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_event(&mut self, ctx: &ConnectionContext, _now_ms: u64, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => match msg.get_payload_bincode::<LayersSpreadRouterSyncMsg>() {
                Ok(LayersSpreadRouterSyncMsg::Sync(sync)) => {
                    if let Some(metric) = self.metric.clone() {
                        log::debug!("[LayersSpreadRouterHander {} {}/{}] on received RouterSync", ctx.local_node_id, ctx.remote_node_id, ctx.conn_id);
                        self.router.apply_sync(ctx.conn_id, ctx.remote_node_id, metric, sync);
                    } else {
                        log::warn!(
                            "[LayersSpreadRouterHander {} {}/{}] on received RouterSync but metric empty",
                            ctx.local_node_id,
                            ctx.remote_node_id,
                            ctx.conn_id
                        );
                        self.wait_sync = Some(sync);
                    }
                }
                Err(err) => {
                    log::error!(
                        "[LayersSpreadRouterHander {} {}/{}] on received invalid msg {}",
                        ctx.local_node_id,
                        ctx.remote_node_id,
                        ctx.conn_id,
                        err
                    );
                }
            },
            ConnectionEvent::Stats(stats) => {
                log::debug!(
                    "[LayersSpreadRouterHander {} {}/{}] on stats rtt_ms {}",
                    ctx.local_node_id,
                    ctx.remote_node_id,
                    ctx.conn_id,
                    stats.rtt_ms
                );
                let metric = Metric::new(stats.rtt_ms, vec![ctx.remote_node_id, ctx.local_node_id], stats.send_est_kbps);
                self.router.set_direct(ctx.conn_id, ctx.remote_node_id, metric.clone());
                if let Some(sync) = self.wait_sync.take() {
                    //first time => send sync
                    log::info!(
                        "[LayersSpreadRouterHander {} {}/{}] on received stats and has remain sync => apply",
                        ctx.local_node_id,
                        ctx.remote_node_id,
                        ctx.conn_id
                    );
                    self.router.apply_sync(ctx.conn_id, ctx.remote_node_id, metric.clone(), sync);
                }
                self.metric = Some(metric);
            }
        }
    }

    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _event: HE) {}

    fn on_closed(&mut self, ctx: &ConnectionContext, _now_ms: u64) {
        self.router.del_direct(ctx.conn_id);
    }

    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>> {
        None
    }
}

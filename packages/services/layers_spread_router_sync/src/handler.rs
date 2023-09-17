use crate::mgs::{LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, LayersSpreadRouterSyncMsg};
use crate::FAST_PATH_ROUTE_SERVICE_ID;
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::RouteRule;
use layers_spread_router::{Metric, RouterSync, SharedRouter};
use network::behaviour::ConnectionHandler;
use network::msg::TransportMsg;
use network::transport::ConnectionEvent;
use network::ConnectionAgent;

pub struct LayersSpreadRouterSyncHandler {
    router: SharedRouter,
    wait_sync: Option<RouterSync>,
    metric: Option<Metric>,
}

impl LayersSpreadRouterSyncHandler {
    pub fn new(router: SharedRouter) -> Self {
        Self {
            router,
            wait_sync: None,
            metric: None,
        }
    }

    fn send_sync<BE, HE>(&mut self, agent: &ConnectionAgent<BE, HE>)
    where
        BE: From<LayersSpreadRouterSyncBehaviorEvent> + TryInto<LayersSpreadRouterSyncBehaviorEvent> + Send + Sync + 'static,
        HE: From<LayersSpreadRouterSyncHandlerEvent> + TryInto<LayersSpreadRouterSyncHandlerEvent> + Send + Sync + 'static,
    {
        let sync = self.router.create_sync(agent.remote_node_id());
        log::debug!("[FastPathRouteHandler {} {}/{}] send RouterSync", agent.local_node_id(), agent.remote_node_id(), agent.conn_id());
        agent.send_net(TransportMsg::build_reliable(
            FAST_PATH_ROUTE_SERVICE_ID,
            RouteRule::Direct,
            0,
            bincode::serialize(&LayersSpreadRouterSyncMsg::Sync(sync)).unwrap(),
        ));
    }
}

impl<BE, HE> ConnectionHandler<BE, HE> for LayersSpreadRouterSyncHandler
where
    BE: From<LayersSpreadRouterSyncBehaviorEvent> + TryInto<LayersSpreadRouterSyncBehaviorEvent> + Send + Sync + 'static,
    HE: From<LayersSpreadRouterSyncHandlerEvent> + TryInto<LayersSpreadRouterSyncHandlerEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE>) {
        self.send_sync(agent);
    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE>, _ts_ms: u64, _interal_ms: u64) {
        self.send_sync(agent);
    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => match msg.get_payload_bincode::<LayersSpreadRouterSyncMsg>() {
                Ok(LayersSpreadRouterSyncMsg::Sync(sync)) => {
                    if let Some(metric) = self.metric.clone() {
                        log::debug!("[FastPathRouteHandler {} {}/{}] on received RouterSync", agent.local_node_id(), agent.remote_node_id(), agent.conn_id());
                        self.router.apply_sync(agent.conn_id(), agent.remote_node_id(), metric, sync);
                    } else {
                        log::warn!(
                            "[FastPathRouteHandler {} {}/{}] on received RouterSync but metric empty",
                            agent.local_node_id(),
                            agent.remote_node_id(),
                            agent.conn_id()
                        );
                        self.wait_sync = Some(sync);
                    }
                }
                Err(err) => {
                    log::error!(
                        "[FastPathRouteHandler {} {}/{}] on received invalid msg {}",
                        agent.local_node_id(),
                        agent.remote_node_id(),
                        agent.conn_id(),
                        err
                    );
                }
            },
            ConnectionEvent::Stats(stats) => {
                log::debug!(
                    "[FastPathRouteHandler {} {}/{}] on stats rtt_ms {}",
                    agent.local_node_id(),
                    agent.remote_node_id(),
                    agent.conn_id(),
                    stats.rtt_ms
                );
                let metric = Metric::new(stats.rtt_ms, vec![agent.remote_node_id(), agent.local_node_id()], stats.send_est_kbps);
                self.router.set_direct(agent.conn_id(), agent.remote_node_id(), metric.clone());
                if let Some(sync) = self.wait_sync.take() {
                    //first time => send sync
                    log::debug!(
                        "[FastPathRouteHandler {} {}/{}] on received stats and has remain sync => apply",
                        agent.local_node_id(),
                        agent.remote_node_id(),
                        agent.conn_id()
                    );
                    self.router.apply_sync(agent.conn_id(), agent.remote_node_id(), metric.clone(), sync);
                }
                self.metric = Some(metric);
            }
        }
    }

    fn on_other_handler_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _event: HE) {}

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE>) {
        self.router.del_direct(agent.conn_id());
    }
}

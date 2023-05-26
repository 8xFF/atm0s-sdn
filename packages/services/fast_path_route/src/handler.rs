use crate::mgs::{FastPathRouteBehaviorEvent, FastPathRouteHandlerEvent, FastPathRouteMsg};
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::ConnectionHandler;
use network::transport::{ConnectionEvent, ConnectionMsg, ConnectionStats};
use network::ConnectionAgent;
use router::{Metric, RouterSync, SharedRouter};

pub struct FastPathRouteHandler {
    router: SharedRouter,
    wait_sync: Option<RouterSync>,
    metric: Option<Metric>,
}

impl FastPathRouteHandler {
    pub fn new(router: SharedRouter) -> Self {
        Self {
            router,
            wait_sync: None,
            metric: None,
        }
    }

    fn send_sync<BE, HE, MSG>(&mut self, agent: &ConnectionAgent<BE, HE, MSG>)
    where
        BE: From<FastPathRouteBehaviorEvent>
            + TryInto<FastPathRouteBehaviorEvent>
            + Send
            + Sync
            + 'static,
        HE: From<FastPathRouteHandlerEvent>
            + TryInto<FastPathRouteHandlerEvent>
            + Send
            + Sync
            + 'static,
        MSG: From<FastPathRouteMsg> + TryInto<FastPathRouteMsg> + Send + Sync + 'static,
    {
        let sync = self.router.create_sync(agent.remote_node_id());
        log::debug!(
            "[FastPathRouteHandler {} {}/{}] send RouterSync",
            agent.local_node_id(),
            agent.remote_node_id(),
            agent.conn_id()
        );
        agent.send_net(ConnectionMsg::Reliable {
            stream_id: 0,
            data: FastPathRouteMsg::Sync(sync).into(),
        });
    }
}

impl<BE, HE, MSG> ConnectionHandler<BE, HE, MSG> for FastPathRouteHandler
where
    BE: From<FastPathRouteBehaviorEvent>
        + TryInto<FastPathRouteBehaviorEvent>
        + Send
        + Sync
        + 'static,
    HE: From<FastPathRouteHandlerEvent>
        + TryInto<FastPathRouteHandlerEvent>
        + Send
        + Sync
        + 'static,
    MSG: From<FastPathRouteMsg> + TryInto<FastPathRouteMsg> + Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {
        self.send_sync(agent);
    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64) {
        self.send_sync(agent);
    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>) {
        match event {
            ConnectionEvent::Msg { service_id, msg } => {
                match msg {
                    ConnectionMsg::Reliable { stream_id, data } => {
                        if let Ok(data) = data.try_into() {
                            match data {
                                FastPathRouteMsg::Sync(sync) => {
                                    if let Some(metric) = self.metric.clone() {
                                        log::debug!("[FastPathRouteHandler {} {}/{}] on received RouterSync", agent.local_node_id(), agent.remote_node_id(), agent.conn_id());
                                        self.router.apply_sync(
                                            agent.conn_id(),
                                            agent.remote_node_id(),
                                            metric,
                                            sync,
                                        );
                                    } else {
                                        log::warn!("[FastPathRouteHandler {} {}/{}] on received RouterSync but metric empty", agent.local_node_id(), agent.remote_node_id(), agent.conn_id());
                                        self.wait_sync = Some(sync);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            ConnectionEvent::Stats(stats) => {
                log::debug!(
                    "[FastPathRouteHandler {} {}/{}] on stats rtt_ms {}",
                    agent.local_node_id(),
                    agent.remote_node_id(),
                    agent.conn_id(),
                    stats.rtt_ms
                );
                let metric = Metric::new(
                    stats.rtt_ms,
                    vec![agent.remote_node_id(), agent.local_node_id()],
                    stats.send_est_kbps,
                );
                self.router
                    .set_direct(agent.conn_id(), agent.remote_node_id(), metric.clone());
                if let Some(sync) = self.wait_sync.take() {
                    //first time => send sync
                    log::debug!("[FastPathRouteHandler {} {}/{}] on received stats and has remain sync => apply", agent.local_node_id(), agent.remote_node_id(), agent.conn_id());
                    self.router.apply_sync(
                        agent.conn_id(),
                        agent.remote_node_id(),
                        metric.clone(),
                        sync,
                    );
                }
                self.metric = Some(metric);
            }
        }
    }

    fn on_other_handler_event(
        &mut self,
        agent: &ConnectionAgent<BE, HE, MSG>,
        from_node: NodeId,
        from_conn: ConnId,
        event: HE,
    ) {
    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE) {}

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {
        self.router.del_direct(agent.conn_id());
    }
}

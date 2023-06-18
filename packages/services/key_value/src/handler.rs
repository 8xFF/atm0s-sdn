use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::ConnectionHandler;
use network::transport::{ConnectionEvent, ConnectionMsg};
use network::{ConnectionAgent, CrossHandlerRoute};
use router::SharedRouter;

pub struct KeyValueConnectionHandler {
    router: SharedRouter,
}

impl KeyValueConnectionHandler {
    pub fn new(router: SharedRouter) -> Self {
        Self { router }
    }
}

impl<BE, HE, Msg> ConnectionHandler<BE, HE, Msg> for KeyValueConnectionHandler
where
    HE: Send + Sync + 'static,
    BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
    Msg: From<KeyValueMsg> + TryInto<KeyValueMsg> + Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, Msg>) {}

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, ts_ms: u64, interal_ms: u64) {}

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, event: ConnectionEvent<Msg>) {
        match event {
            ConnectionEvent::Msg { route, ttl, msg, service_id } => match msg {
                ConnectionMsg::Reliable { data, stream_id } => {
                    if let Ok(kv_msg) = data.try_into() {
                        agent.send_behavior(KeyValueBehaviorEvent::FromNode(kv_msg).into());
                    }
                }
                ConnectionMsg::Unreliable { .. } => {}
            },
            ConnectionEvent::Stats(_) => {}
        }
    }

    fn on_other_handler_event(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, from_node: NodeId, from_conn: ConnId, event: HE) {}

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, event: HE) {}

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, Msg>) {}
}

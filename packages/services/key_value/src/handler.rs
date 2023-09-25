use std::sync::Arc;

use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::RouterTable;
use network::behaviour::ConnectionHandler;
use network::transport::ConnectionEvent;
use network::ConnectionAgent;

pub struct KeyValueConnectionHandler {
    #[allow(unused)]
    router: Arc<dyn RouterTable>,
}

impl KeyValueConnectionHandler {
    pub fn new(router: Arc<dyn RouterTable>) -> Self {
        Self { router }
    }
}

impl<BE, HE> ConnectionHandler<BE, HE> for KeyValueConnectionHandler
where
    HE: Send + Sync + 'static,
    BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, _agent: &ConnectionAgent<BE, HE>) {}

    fn on_tick(&mut self, _agent: &ConnectionAgent<BE, HE>, _ts_ms: u64, _interal_ms: u64) {}

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => match msg.get_payload_bincode::<KeyValueMsg>() {
                Ok(kv_msg) => {
                    agent.send_behavior(KeyValueBehaviorEvent::FromNode(kv_msg).into());
                }
                Err(e) => {
                    log::error!("Error on get_payload_bincode: {:?}", e);
                }
            },
            ConnectionEvent::Stats(_) => {}
        }
    }

    fn on_other_handler_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _event: HE) {}

    fn on_closed(&mut self, _agent: &ConnectionAgent<BE, HE>) {}
}

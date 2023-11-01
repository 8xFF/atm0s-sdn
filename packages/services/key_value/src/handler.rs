use std::collections::VecDeque;

use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{ConnectionHandler, ConnectionContext, ConnectionHandlerAction};
use network::transport::ConnectionEvent;

pub struct KeyValueConnectionHandler<BE, HE> {
    ouputs: VecDeque<ConnectionHandlerAction<BE, HE>>,
}

impl<BE, HE> KeyValueConnectionHandler<BE, HE> {
    pub fn new() -> Self {
        Self {
            ouputs: VecDeque::new(),
        }
    }
}

impl<BE, HE> ConnectionHandler<BE, HE> for KeyValueConnectionHandler<BE, HE>
where
    HE: Send + Sync + 'static,
    BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_tick(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _interal_ms: u64) {}
    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => match msg.get_payload_bincode::<KeyValueMsg>() {
                Ok(kv_msg) => {
                    self.ouputs.push_back(ConnectionHandlerAction::ToBehaviour(KeyValueBehaviorEvent::FromNode(msg.header, kv_msg).into()));
                }
                Err(e) => {
                    log::error!("Error on get_payload_bincode: {:?}", e);
                }
            },
            ConnectionEvent::Stats(_) => {}
        }
    }

    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _event: HE) {}

    fn on_closed(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>> {
        self.ouputs.pop_front()
    }
}

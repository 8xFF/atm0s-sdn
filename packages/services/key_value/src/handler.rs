use std::collections::VecDeque;

use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction};
use atm0s_sdn_network::transport::ConnectionEvent;

pub struct KeyValueConnectionHandler<BE, HE> {
    ouputs: VecDeque<ConnectionHandlerAction<BE, HE>>,
}

impl<BE, HE> KeyValueConnectionHandler<BE, HE> {
    pub fn new() -> Self {
        Self { ouputs: VecDeque::new() }
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
            ConnectionEvent::Msg(msg) => {
                if let Some(from) = msg.header.from_node {
                    match msg.get_payload_bincode::<KeyValueMsg>() {
                        Ok(kv_msg) => {
                            log::debug!("[KeyValueConnectionHandler {}] forward to Behaviour {:?}", _ctx.local_node_id, kv_msg);
                            self.ouputs.push_back(ConnectionHandlerAction::ToBehaviour(KeyValueBehaviorEvent::FromNode(from, kv_msg).into()));
                        }
                        Err(e) => {
                            log::error!("Error on get_payload_bincode: {:?}", e);
                        }
                    }
                }
            }
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

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::{msg::SimpleLocalEvent, KeyValueBehaviorEvent, KeyValueMsg, KEY_VALUE_SERVICE_ID};
    use atm0s_sdn_identity::ConnId;
    use atm0s_sdn_network::{
        behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction},
        convert_enum,
        msg::{MsgHeader, TransportMsg},
        transport::ConnectionEvent,
    };
    use atm0s_sdn_router::RouteRule;
    use atm0s_sdn_utils::awaker::MockAwaker;

    use super::KeyValueConnectionHandler;

    #[derive(convert_enum::From, convert_enum::TryInto, PartialEq, Debug)]
    enum BE {
        KeyValueBehaviorEvent(KeyValueBehaviorEvent),
    }

    #[test]
    fn should_not_forward_invalid_msg() {
        let mut handler = KeyValueConnectionHandler::<KeyValueBehaviorEvent, ()>::new();
        let ctx = ConnectionContext {
            conn_id: ConnId::from_in(0, 0),
            local_node_id: 0,
            remote_node_id: 1,
            service_id: KEY_VALUE_SERVICE_ID,
            awaker: Arc::new(MockAwaker::default()),
        };
        let mut trans_msg = TransportMsg::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::Direct, 0, 0, &vec![]);
        trans_msg.header.from_node = Some(1);
        handler.on_event(&ctx, 0, ConnectionEvent::Msg(trans_msg));
        assert_eq!(handler.pop_action(), None);
    }

    #[test]
    fn should_forward_valid_msg() {
        let mut handler = KeyValueConnectionHandler::<KeyValueBehaviorEvent, ()>::new();
        let ctx = ConnectionContext {
            conn_id: ConnId::from_in(0, 0),
            local_node_id: 0,
            remote_node_id: 1,
            service_id: KEY_VALUE_SERVICE_ID,
            awaker: Arc::new(MockAwaker::default()),
        };

        let header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::Direct).set_from_node(Some(1));
        let msg = KeyValueMsg::SimpleLocal(SimpleLocalEvent::SetAck(0, 0, 0, false));
        let trans_msg = TransportMsg::from_payload_bincode(header, &msg);
        handler.on_event(&ctx, 0, ConnectionEvent::Msg(trans_msg));
        assert_eq!(handler.pop_action(), Some(ConnectionHandlerAction::ToBehaviour(KeyValueBehaviorEvent::FromNode(1, msg))));
    }

    #[test]
    fn should_forward_valid_msg_but_no_from_node() {
        let mut handler = KeyValueConnectionHandler::<KeyValueBehaviorEvent, ()>::new();
        let ctx = ConnectionContext {
            conn_id: ConnId::from_in(0, 0),
            local_node_id: 0,
            remote_node_id: 1,
            service_id: KEY_VALUE_SERVICE_ID,
            awaker: Arc::new(MockAwaker::default()),
        };

        let header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::Direct).set_from_node(None);
        let msg = KeyValueMsg::SimpleLocal(SimpleLocalEvent::SetAck(0, 0, 0, false));
        let trans_msg = TransportMsg::from_payload_bincode(header, &msg);
        handler.on_event(&ctx, 0, ConnectionEvent::Msg(trans_msg));
        assert_eq!(handler.pop_action(), None);
    }
}

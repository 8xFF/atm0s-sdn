use crate::handler::KeyValueConnectionHandler;
use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg, KeyValueSdkEvent};
use crate::{ExternalControl, KEY_VALUE_SERVICE_ID};
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction};
use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
use atm0s_sdn_network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError};
use std::collections::VecDeque;
use std::sync::Arc;

use self::hashmap_local::HashmapLocalStorage;
use self::hashmap_remote::HashmapRemoteStorage;
use self::simple_local::SimpleLocalStorage;
use self::simple_remote::SimpleRemoteStorage;

mod event_acks;
mod hashmap_local;
mod hashmap_remote;
mod sdk;
mod simple_local;
mod simple_remote;

pub use sdk::KeyValueSdk;

#[allow(unused)]
pub struct KeyValueBehavior<HE, SE> {
    node_id: NodeId,
    simple_remote: SimpleRemoteStorage,
    simple_local: SimpleLocalStorage,
    hashmap_remote: HashmapRemoteStorage,
    hashmap_local: HashmapLocalStorage,
    outputs: VecDeque<NetworkBehaviorAction<HE, SE>>,
    external: Option<Box<dyn ExternalControl>>,
}

impl<HE, SE> KeyValueBehavior<HE, SE>
where
    HE: Send + Sync + 'static,
    SE: From<KeyValueSdkEvent> + TryInto<KeyValueSdkEvent> + Send + Sync + 'static,
{
    #[allow(unused)]
    pub fn new(node_id: NodeId, sync_each_ms: u64, external: Option<Box<dyn ExternalControl>>) -> Self {
        log::info!("[KeyValueBehaviour {}] created with sync_each_ms {}", node_id, sync_each_ms);
        Self {
            node_id,
            simple_remote: SimpleRemoteStorage::new(),
            simple_local: SimpleLocalStorage::new(sync_each_ms),
            hashmap_remote: HashmapRemoteStorage::new(node_id),
            hashmap_local: HashmapLocalStorage::new(sync_each_ms),
            outputs: VecDeque::new(),
            external,
        }
    }

    fn pop_all_events<BE>(&mut self, _ctx: &BehaviorContext, now_ms: u64)
    where
        BE: Send + Sync + 'static,
    {
        while let Some(action) = self.simple_remote.pop_action(now_ms) {
            log::debug!("[KeyValueBehavior {}] pop_all_events simple remote: {:?}", self.node_id, action);
            let header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, action.1).set_from_node(Some(self.node_id));
            self.outputs
                .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::SimpleLocal(action.0))));
        }

        while let Some(action) = self.simple_local.pop_action() {
            match action {
                simple_local::LocalStorageAction::SendNet(msg, route) => {
                    log::debug!("[KeyValueBehavior {}] pop_all_events simple local: {:?}", self.node_id, msg);
                    let header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, route).set_from_node(Some(self.node_id));
                    self.outputs
                        .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::SimpleRemote(msg))));
                }
                simple_local::LocalStorageAction::LocalOnChanged(service_id, uuid, key, value, version, source) => {
                    if service_id == KEY_VALUE_SERVICE_ID {
                        if let Some(external) = &self.external {
                            external.on_event(KeyValueSdkEvent::OnKeyChanged(uuid, key, value, version, source));
                        }
                    } else {
                        self.outputs.push_back(NetworkBehaviorAction::ToSdkService(
                            service_id,
                            KeyValueSdkEvent::OnKeyChanged(uuid, key, value, version, source).into(),
                        ));
                    }
                }
                simple_local::LocalStorageAction::LocalOnGet(service_id, uuid, key, res) => {
                    if service_id == KEY_VALUE_SERVICE_ID {
                        if let Some(external) = &self.external {
                            external.on_event(KeyValueSdkEvent::OnGet(uuid, key, res));
                        }
                    } else {
                        self.outputs.push_back(NetworkBehaviorAction::ToSdkService(service_id, KeyValueSdkEvent::OnGet(uuid, key, res).into()));
                    }
                }
            }
        }

        while let Some(action) = self.hashmap_remote.pop_action(now_ms) {
            log::debug!("[KeyValueBehavior {}] pop_all_events hashmap remote: {:?}", self.node_id, action);
            let header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, action.1).set_from_node(Some(self.node_id));
            self.outputs
                .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::HashmapLocal(action.0))));
        }

        while let Some(action) = self.hashmap_local.pop_action() {
            match action {
                hashmap_local::LocalStorageAction::SendNet(msg, route) => {
                    log::debug!("[KeyValueBehavior {}] pop_all_events hashmap local: {:?}", self.node_id, msg);
                    let header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, route).set_from_node(Some(self.node_id));
                    self.outputs
                        .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::HashmapRemote(msg))));
                }
                hashmap_local::LocalStorageAction::LocalOnChanged(service_id, uuid, key, sub_key, value, version, source) => {
                    if service_id == KEY_VALUE_SERVICE_ID {
                        if let Some(external) = &self.external {
                            external.on_event(KeyValueSdkEvent::OnKeyHChanged(uuid, key, sub_key, value, version, source));
                        }
                    } else {
                        self.outputs.push_back(NetworkBehaviorAction::ToSdkService(
                            service_id,
                            KeyValueSdkEvent::OnKeyHChanged(uuid, key, sub_key, value, version, source).into(),
                        ));
                    }
                }
                hashmap_local::LocalStorageAction::LocalOnGet(service_id, uuid, key, res) => {
                    if service_id == KEY_VALUE_SERVICE_ID {
                        if let Some(external) = &self.external {
                            external.on_event(KeyValueSdkEvent::OnGetH(uuid, key, res));
                        }
                    } else {
                        self.outputs.push_back(NetworkBehaviorAction::ToSdkService(service_id, KeyValueSdkEvent::OnGetH(uuid, key, res).into()));
                    }
                }
            }
        }
    }

    fn process_key_value_msg<BE>(&mut self, ctx: &BehaviorContext, now_ms: u64, from: NodeId, msg: KeyValueMsg)
    where
        BE: Send + Sync + 'static,
    {
        match msg {
            KeyValueMsg::SimpleRemote(msg) => {
                log::debug!("[KeyValueBehavior {}] process_key_value_msg simple remote: {:?} from {}", self.node_id, msg, from);
                self.simple_remote.on_event(now_ms, from, msg);
                self.pop_all_events::<BE>(ctx, now_ms);
            }
            KeyValueMsg::SimpleLocal(msg) => {
                log::debug!("[KeyValueBehavior {}] process_key_value_msg simple local: {:?} from {}", self.node_id, msg, from);
                self.simple_local.on_event(from, msg);
                self.pop_all_events::<BE>(ctx, now_ms);
            }
            KeyValueMsg::HashmapRemote(msg) => {
                log::debug!("[KeyValueBehavior {}] process_key_value_msg hashmap remote: {:?} from {}", self.node_id, msg, from);
                self.hashmap_remote.on_event(now_ms, from, msg);
                self.pop_all_events::<BE>(ctx, now_ms);
            }
            KeyValueMsg::HashmapLocal(msg) => {
                log::debug!("[KeyValueBehavior {}] process_key_value_msg hashmap local: {:?} from {}", self.node_id, msg, from);
                self.hashmap_local.on_event(from, msg);
                self.pop_all_events::<BE>(ctx, now_ms);
            }
        }
    }

    fn process_sdk_event(&mut self, _ctx: &BehaviorContext, now_ms: u64, from_service: u8, event: KeyValueSdkEvent) {
        match event {
            KeyValueSdkEvent::Get(req_id, key, timeout_ms) => {
                self.simple_local.get(now_ms, key, req_id, from_service, timeout_ms);
            }
            KeyValueSdkEvent::GetH(req_id, key, timeout_ms) => {
                self.hashmap_local.get(now_ms, key, req_id, from_service, timeout_ms);
            }
            KeyValueSdkEvent::Set(key, value, ex) => {
                self.simple_local.set(now_ms, key, value, ex);
            }
            KeyValueSdkEvent::SetH(key, sub_key, value, ex) => {
                self.hashmap_local.set(now_ms, key, sub_key, value, ex);
            }
            KeyValueSdkEvent::Del(key) => {
                self.simple_local.del(key);
            }
            KeyValueSdkEvent::DelH(key, sub_key) => {
                self.hashmap_local.del(key, sub_key);
            }
            KeyValueSdkEvent::Sub(uuid, key, ex) => {
                self.simple_local.subscribe(key, ex, uuid, from_service);
            }
            KeyValueSdkEvent::SubH(uuid, key, ex) => {
                self.hashmap_local.subscribe(key, ex, uuid, from_service);
            }
            KeyValueSdkEvent::Unsub(uuid, key) => {
                self.simple_local.unsubscribe(key, uuid, from_service);
            }
            KeyValueSdkEvent::UnsubH(uuid, key) => {
                self.hashmap_local.unsubscribe(key, uuid, from_service);
            }
            _ => {}
        }
    }
}

#[allow(unused)]
impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for KeyValueBehavior<HE, SE>
where
    BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
    HE: Send + Sync + 'static,
    SE: From<KeyValueSdkEvent> + TryInto<KeyValueSdkEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        KEY_VALUE_SERVICE_ID
    }

    fn on_tick(&mut self, ctx: &BehaviorContext, now_ms: u64, interal_ms: u64) {
        log::trace!("[KeyValueBehavior {}] on_tick ts_ms {}, interal_ms {}", self.node_id, now_ms, interal_ms);
        self.simple_remote.tick(now_ms);
        self.simple_local.tick(now_ms);
        self.hashmap_remote.tick(now_ms);
        self.hashmap_local.tick(now_ms);
        self.pop_all_events::<BE>(ctx, now_ms);
    }

    fn on_awake(&mut self, ctx: &BehaviorContext, now_ms: u64) {
        while let Some(external) = &self.external {
            if let Some(event) = external.pop_action() {
                log::info!("[KeyValueBehavior {}] external event: {:?}", self.node_id, event);
                self.process_sdk_event(ctx, now_ms, KEY_VALUE_SERVICE_ID, event);
            } else {
                break;
            }
        }
        self.pop_all_events::<BE>(ctx, now_ms);
    }

    fn check_incoming_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_local_msg(&mut self, ctx: &BehaviorContext, now_ms: u64, msg: TransportMsg) {
        //TODO avoid serialize and deserialize in local
        match msg.get_payload_bincode::<KeyValueMsg>() {
            Ok(kv_msg) => {
                log::debug!("[KeyValueBehavior {}] on_local_msg: {:?}", self.node_id, kv_msg);
                self.process_key_value_msg::<BE>(ctx, now_ms, self.node_id, kv_msg);
            }
            Err(e) => {
                log::error!("Error on get_payload_bincode: {:?}", e);
            }
        }
    }

    fn on_sdk_msg(&mut self, ctx: &BehaviorContext, now_ms: u64, from_service: u8, event: SE) {
        if let Ok(event) = event.try_into() {
            self.process_sdk_event(ctx, now_ms, from_service, event);
            self.pop_all_events::<BE>(ctx, now_ms);
        } else {
            debug_assert!(false, "Invalid event")
        }
    }

    fn on_incoming_connection_connected(&mut self, ctx: &BehaviorContext, now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(&mut self, ctx: &BehaviorContext, now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) {}

    fn on_outgoing_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) {}

    fn on_outgoing_connection_error(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId, event: BE) {
        if let Ok(msg) = event.try_into() {
            match msg {
                KeyValueBehaviorEvent::FromNode(from, msg) => {
                    self.process_key_value_msg::<BE>(ctx, now_ms, from, msg);
                }
                _ => {}
            }
        }
    }

    fn on_started(&mut self, ctx: &BehaviorContext, now_ms: u64) {
        log::info!("[KeyValueBehavior {}] on_started", self.node_id);
        if let Some(external) = &self.external {
            external.set_awaker(ctx.awaker.clone());
        }
    }

    fn on_stopped(&mut self, ctx: &BehaviorContext, now_ms: u64) {
        log::info!("[KeyValueBehavior {}] on_stopped", self.node_id);
    }

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        self.outputs.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use atm0s_sdn_identity::ConnId;
    use atm0s_sdn_network::{
        behaviour::{BehaviorContext, NetworkBehavior, NetworkBehaviorAction},
        msg::{MsgHeader, TransportMsg},
    };
    use atm0s_sdn_router::RouteRule;
    use atm0s_sdn_utils::awaker::MockAwaker;

    use crate::{
        msg::{HashmapLocalEvent, HashmapRemoteEvent, KeyValueSdkEvent, SimpleLocalEvent, SimpleRemoteEvent},
        KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueMsg, MockExternalControl, KEY_VALUE_SERVICE_ID,
    };

    type BE = KeyValueBehaviorEvent;
    type HE = KeyValueHandlerEvent;
    type SE = KeyValueSdkEvent;

    #[test]
    fn set_simple_key_send_to_net() {
        let local_node_id = 1;
        let remote_node_id = 2;
        let sync_ms = 10000;
        let key = 1000;
        let mut behaviour = super::KeyValueBehavior::<HE, SE>::new(local_node_id, sync_ms, None);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        behaviour.on_sdk_msg(&ctx, 0, 0, KeyValueSdkEvent::Set(key, vec![1], None));

        // after handle sdk this should send Set to Key(1000)
        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Set(0, key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        behaviour.on_tick(&ctx, 100, 100);

        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Set(1, key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            200,
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::SimpleLocal(SimpleLocalEvent::SetAck(1, 1000, 0, true))),
        );

        behaviour.on_tick(&ctx, 200, 100);
        assert_eq!(behaviour.pop_action(), None);

        // after del should send Del to Key(1000)
        behaviour.on_sdk_msg(&ctx, 0, 0, KeyValueSdkEvent::Del(key));

        // after awake this should send Del to Key(1000)
        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Del(2, key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        behaviour.on_tick(&ctx, 300, 100);

        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Del(3, key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            400,
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::SimpleLocal(SimpleLocalEvent::DelAck(3, 1000, Some(0)))),
        );

        behaviour.on_tick(&ctx, 400, 100);
        assert_eq!(behaviour.pop_action(), None);
    }

    #[test]
    fn sdk_hash_set_del_should_fire_event() {
        let local_node_id = 1;
        let remote_node_id = 2;
        let sync_ms = 10000;
        let key = 1000;
        let sub_key = 111;
        let mut behaviour = super::KeyValueBehavior::<HE, SE>::new(local_node_id, sync_ms, None);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        // now set key should be awake and output Set command
        behaviour.on_sdk_msg(&ctx, 0, 0, KeyValueSdkEvent::SetH(key, sub_key, vec![1], None));

        // after awake this should send Set to Key(1000)
        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Set(0, key, sub_key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        behaviour.on_tick(&ctx, 100, 100);

        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Set(1, key, sub_key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            200,
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::HashmapLocal(HashmapLocalEvent::SetAck(1, key, sub_key, 0, true))),
        );

        behaviour.on_tick(&ctx, 200, 100);
        assert_eq!(behaviour.pop_action(), None);

        // after del should send Del to Key(1000)
        behaviour.on_sdk_msg(&ctx, 0, 0, KeyValueSdkEvent::DelH(key, sub_key));

        // after awake this should send Del to Key(1000)
        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Del(2, key, sub_key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        behaviour.on_tick(&ctx, 200, 100);

        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Del(3, key, sub_key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            400,
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::HashmapLocal(HashmapLocalEvent::DelAck(3, key, sub_key, Some(0)))),
        );

        behaviour.on_tick(&ctx, 400, 100);
        assert_eq!(behaviour.pop_action(), None);
    }

    #[test]
    fn remote_set_del_should_fire_ack() {
        let local_node_id = 1;
        let remote_node_id = 2;
        let sync_ms = 10000;
        let key = 1000;
        let mut behaviour = super::KeyValueBehavior::<HE, SE>::new(local_node_id, sync_ms, None);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        // received Simple Set
        behaviour.on_handler_event(
            &ctx,
            0,
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Set(0, key, vec![1], 0, None))),
        );

        // should send ack
        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToNode(remote_node_id)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleLocal(SimpleLocalEvent::SetAck(0, key, 0, true)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));
    }

    #[test]
    fn remote_hash_set_del_should_fire_ack() {
        let local_node_id = 1;
        let remote_node_id = 2;
        let sync_ms = 10000;
        let key = 1000;
        let sub_key = 111;
        let mut behaviour = super::KeyValueBehavior::<HE, SE>::new(local_node_id, sync_ms, None);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        // received Simple Set
        behaviour.on_handler_event(
            &ctx,
            0,
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Set(0, key, sub_key, vec![1], 0, None))),
        );

        // should send ack
        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToNode(remote_node_id)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapLocal(HashmapLocalEvent::SetAck(0, key, sub_key, 0, true)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));
    }

    #[test]
    fn sdk_simple_sub_should_fire_event() {
        let local_node_id = 1;
        let sync_ms = 10000;
        let key = 1000;
        let mut behaviour = super::KeyValueBehavior::<HE, SE>::new(local_node_id, sync_ms, None);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        // now set key should be awake and output Set command
        behaviour.on_sdk_msg(&ctx, 0, 0, KeyValueSdkEvent::Sub(0, key, None));

        // after awake this should send Set to Key(1000)
        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Sub(0, key, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));
    }

    #[test]
    fn test_on_awake() {
        let local_node_id = 1;
        let sync_ms = 10000;
        let key = 1000;
        let mut mock_external_control = Box::new(MockExternalControl::default());
        let mut external_action: Vec<Option<KeyValueSdkEvent>> = vec![None, Some(KeyValueSdkEvent::Sub(0, key, None))];
        mock_external_control.expect_pop_action().returning(move || external_action.pop().unwrap());

        let mut behaviour = super::KeyValueBehavior::<HE, SE>::new(local_node_id, sync_ms, Some(mock_external_control));
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_awake(&ctx, 0);

        let expected_header = MsgHeader::build(KEY_VALUE_SERVICE_ID, KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32)).set_from_node(Some(local_node_id));

        assert_eq!(
            behaviour.pop_action(),
            Some(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(
                expected_header,
                &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Sub(0, key, None))
            )))
        );
    }
}

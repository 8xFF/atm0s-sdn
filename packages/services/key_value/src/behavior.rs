use crate::handler::KeyValueConnectionHandler;
use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg, KeyValueSdkEvent};
use crate::KEY_VALUE_SERVICE_ID;
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction};
use network::msg::{MsgHeader, TransportMsg};
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use utils::awaker::MockAwaker;
use utils::Timer;

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
    simple_local: Arc<RwLock<SimpleLocalStorage>>,
    hashmap_remote: HashmapRemoteStorage,
    hashmap_local: Arc<RwLock<HashmapLocalStorage>>,
    outputs: VecDeque<NetworkBehaviorAction<HE, SE>>,
}

impl<HE, SE> KeyValueBehavior<HE, SE>
where
    HE: Send + Sync + 'static,
    SE: Send + Sync + 'static,
{
    #[allow(unused)]
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>, sync_each_ms: u64) -> (Self, sdk::KeyValueSdk) {
        log::info!("[KeyValueBehaviour {}] created with sync_each_ms {}", node_id, sync_each_ms);
        let default_awake = Arc::new(MockAwaker::default());
        let simple_local = Arc::new(RwLock::new(SimpleLocalStorage::new(default_awake.clone(), sync_each_ms)));
        let hashmap_local = Arc::new(RwLock::new(HashmapLocalStorage::new(default_awake.clone(), sync_each_ms)));
        let sdk = sdk::KeyValueSdk::new(simple_local.clone(), hashmap_local.clone(), timer);

        let sdk_c = sdk.clone();
        (
            Self {
                node_id,
                simple_remote: SimpleRemoteStorage::new(),
                simple_local,
                hashmap_remote: HashmapRemoteStorage::new(node_id),
                hashmap_local,
                outputs: VecDeque::new(),
            },
            sdk,
        )
    }

    fn pop_all_events<BE>(&mut self, _ctx: &BehaviorContext)
    where
        BE: Send + Sync + 'static,
    {
        while let Some(action) = self.simple_remote.pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events simple remote: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            self.outputs
                .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::SimpleLocal(action.0))));
        }

        while let Some(action) = self.simple_local.write().pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events simple local: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            self.outputs
                .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::SimpleRemote(action.0))));
        }

        while let Some(action) = self.hashmap_remote.pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events hashmap remote: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            self.outputs
                .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::HashmapLocal(action.0))));
        }

        while let Some(action) = self.hashmap_local.write().pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events hashmap local: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            self.outputs
                .push_back(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &KeyValueMsg::HashmapRemote(action.0))));
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
                self.pop_all_events::<BE>(ctx);
            }
            KeyValueMsg::SimpleLocal(msg) => {
                log::debug!("[KeyValueBehavior {}] process_key_value_msg simple local: {:?} from {}", self.node_id, msg, from);
                self.simple_local.write().on_event(from, msg);
                self.pop_all_events::<BE>(ctx);
            }
            KeyValueMsg::HashmapRemote(msg) => {
                log::debug!("[KeyValueBehavior {}] process_key_value_msg hashmap remote: {:?} from {}", self.node_id, msg, from);
                self.hashmap_remote.on_event(now_ms, from, msg);
                self.pop_all_events::<BE>(ctx);
            }
            KeyValueMsg::HashmapLocal(msg) => {
                log::debug!("[KeyValueBehavior {}] process_key_value_msg hashmap local: {:?} from {}", self.node_id, msg, from);
                self.hashmap_local.write().on_event(from, msg);
                self.pop_all_events::<BE>(ctx);
            }
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
        self.simple_local.write().tick(now_ms);
        self.hashmap_remote.tick(now_ms);
        self.hashmap_local.write().tick(now_ms);
        self.pop_all_events::<BE>(ctx);
    }

    fn on_awake(&mut self, ctx: &BehaviorContext, now_ms: u64) {
        self.pop_all_events::<BE>(ctx);
    }

    fn check_incoming_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId, local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
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
            match event {
                KeyValueSdkEvent::Local(msg) => {}
                KeyValueSdkEvent::FromNode(node_id, msg) => {}
            }
        }
    }

    fn on_incoming_connection_connected(&mut self, ctx: &BehaviorContext, now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        ctx: &BehaviorContext,
        now_ms: u64,
        conn: Arc<dyn ConnectionSender>,
        local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) {}

    fn on_outgoing_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) {}

    fn on_outgoing_connection_error(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: Option<ConnId>, local_uuid: TransportOutgoingLocalUuid, err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId, event: BE) {
        log::info!("received event");
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
        self.simple_local.write().change_awake_notify(ctx.awaker.clone());
        self.hashmap_local.write().change_awake_notify(ctx.awaker.clone());
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

    use bluesea_identity::ConnId;
    use bluesea_router::RouteRule;
    use network::{
        behaviour::{BehaviorContext, NetworkBehavior, NetworkBehaviorAction},
        msg::{MsgHeader, TransportMsg},
    };
    use utils::{awaker::MockAwaker, MockTimer, Timer};

    use crate::{
        msg::{HashmapLocalEvent, HashmapRemoteEvent, SimpleLocalEvent, SimpleRemoteEvent, KeyValueSdkEvent},
        KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueMsg, KEY_VALUE_SERVICE_ID,
    };

    type BE = KeyValueBehaviorEvent;
    type HE = KeyValueHandlerEvent;
    type SE = KeyValueSdkEvent;

    #[test]
    fn sdk_simple_set_del_should_fire_event() {
        let local_node_id = 1;
        let remote_node_id = 2;
        let sync_ms = 10000;
        let key = 1000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::<HE, SE>::new(local_node_id, timer.clone(), sync_ms);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, timer.now_ms());

        // now set key should be awake and output Set command
        sdk.set(key, vec![1], None);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());

        // after awake this should send Set to Key(1000)
        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Set(0, key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        timer.fake(100);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);

        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Set(1, key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            timer.now_ms(),
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::SimpleLocal(SimpleLocalEvent::SetAck(1, 1000, 0, true))),
        );

        timer.fake(200);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);
        assert_eq!(behaviour.pop_action(), None);

        // after del should send Del to Key(1000)
        sdk.del(key);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());

        // after awake this should send Del to Key(1000)
        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Del(2, key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        timer.fake(300);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);

        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Del(3, key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            timer.now_ms(),
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::SimpleLocal(SimpleLocalEvent::DelAck(3, 1000, Some(0)))),
        );

        timer.fake(400);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);
        assert_eq!(behaviour.pop_action(), None);
    }

    #[test]
    fn sdk_hash_set_del_should_fire_event() {
        let local_node_id = 1;
        let remote_node_id = 2;
        let sync_ms = 10000;
        let key = 1000;
        let sub_key = 111;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::<HE, SE>::new(local_node_id, timer.clone(), sync_ms);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, timer.now_ms());

        // now set key should be awake and output Set command
        sdk.hset(key, sub_key, vec![1], None);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());

        // after awake this should send Set to Key(1000)
        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Set(0, key, sub_key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        timer.fake(100);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);

        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Set(1, key, sub_key, vec![1], 0, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            timer.now_ms(),
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::HashmapLocal(HashmapLocalEvent::SetAck(1, key, sub_key, 0, true))),
        );

        timer.fake(200);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);
        assert_eq!(behaviour.pop_action(), None);

        // after del should send Del to Key(1000)
        sdk.hdel(key, sub_key);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());

        // after awake this should send Del to Key(1000)
        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Del(2, key, sub_key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after tick without ack should resend
        timer.fake(300);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);

        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Del(3, key, sub_key, 0)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));

        // after handle ack should not resend
        behaviour.on_handler_event(
            &ctx,
            timer.now_ms(),
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::HashmapLocal(HashmapLocalEvent::DelAck(3, key, sub_key, Some(0)))),
        );

        timer.fake(400);
        behaviour.on_tick(&ctx, timer.now_ms(), 100);
        assert_eq!(behaviour.pop_action(), None);
    }

    #[test]
    fn remote_set_del_should_fire_ack() {
        let local_node_id = 1;
        let remote_node_id = 2;
        let sync_ms = 10000;
        let key = 1000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, _sdk) = super::KeyValueBehavior::<HE, SE>::new(local_node_id, timer.clone(), sync_ms);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, timer.now_ms());

        // received Simple Set
        behaviour.on_handler_event(
            &ctx,
            timer.now_ms(),
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Set(0, key, vec![1], 0, None))),
        );

        // should send ack
        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToNode(remote_node_id), 0);
        expected_header.from_node = Some(local_node_id);
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
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, _sdk) = super::KeyValueBehavior::<HE, SE>::new(local_node_id, timer.clone(), sync_ms);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, timer.now_ms());

        // received Simple Set
        behaviour.on_handler_event(
            &ctx,
            timer.now_ms(),
            remote_node_id,
            ConnId::from_in(0, 0),
            KeyValueBehaviorEvent::FromNode(remote_node_id, KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Set(0, key, sub_key, vec![1], 0, None))),
        );

        // should send ack
        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToNode(remote_node_id), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::HashmapLocal(HashmapLocalEvent::SetAck(0, key, sub_key, 0, true)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));
    }

    #[test]
    fn sdk_simple_sub_should_fire_event() {
        let local_node_id = 1;
        let sync_ms = 10000;
        let key = 1000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::<HE, SE>::new(local_node_id, timer.clone(), sync_ms);
        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        let ctx = BehaviorContext {
            service_id: KEY_VALUE_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, timer.now_ms());

        // now set key should be awake and output Set command
        let _sub = sdk.subscribe(key, None);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());

        // after awake this should send Set to Key(1000)
        let mut expected_header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, RouteRule::ToKey(key as u32), 0);
        expected_header.from_node = Some(local_node_id);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Sub(0, key, None)));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNet(expected_msg)));
    }

    //TODO test after received sub event and set event should send OnSet event
}

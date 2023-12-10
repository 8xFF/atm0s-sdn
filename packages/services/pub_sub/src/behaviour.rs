use std::{collections::VecDeque, marker::PhantomData, sync::Arc};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_key_value::{KeyValueSdkEvent, KEY_VALUE_SERVICE_ID};
use atm0s_sdn_network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::{MsgHeader, TransportMsg},
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid},
};
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::Timer;

use crate::{
    handler::{PubsubServiceConnectionHandler, CONTROL_META_TYPE, FEEDBACK_TYPE},
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{local::LocalRelayAction, logic::PubsubRelayLogicOutput, source_binding::SourceBindingAction, PubsubRelay},
    PubsubSdk, PUBSUB_SERVICE_ID,
};

const KEY_VALUE_TIMEOUT_MS: u64 = 30000;
const KEY_VALUE_SUB_UUID: u64 = 0;

pub struct PubsubServiceBehaviour<BE, HE, SE> {
    _tmp: PhantomData<BE>,
    node_id: NodeId,
    relay: PubsubRelay,
    outputs: VecDeque<NetworkBehaviorAction<HE, SE>>,
}

impl<BE, HE, SE> PubsubServiceBehaviour<BE, HE, SE>
where
    SE: From<KeyValueSdkEvent> + TryInto<KeyValueSdkEvent>,
{
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>) -> (Self, PubsubSdk) {
        let (relay, sdk) = PubsubRelay::new(node_id, timer);
        (
            Self {
                _tmp: Default::default(),
                node_id,
                relay,
                outputs: VecDeque::new(),
            },
            sdk,
        )
    }

    fn pop_all_events(&mut self, _ctx: &BehaviorContext) {
        while let Some((node, conn, action)) = self.relay.pop_logic_action() {
            let msg = match action {
                PubsubRelayLogicOutput::Event(e) => {
                    let header = MsgHeader::build(PUBSUB_SERVICE_ID, PUBSUB_SERVICE_ID, RouteRule::Direct).set_meta(CONTROL_META_TYPE);
                    TransportMsg::from_payload_bincode(header, &e)
                }
                PubsubRelayLogicOutput::Feedback(fb) => {
                    let header = MsgHeader::build(PUBSUB_SERVICE_ID, PUBSUB_SERVICE_ID, RouteRule::Direct).set_meta(FEEDBACK_TYPE);
                    TransportMsg::from_payload_bincode(header, &fb)
                }
            };

            //Should be send to correct conn, if that conn not exits => fallback by finding to origin source node
            if let Some(conn) = conn {
                self.outputs.push_back(NetworkBehaviorAction::ToNetConn(conn, msg));
            } else {
                self.outputs.push_back(NetworkBehaviorAction::ToNetNode(node, msg));
            }
        }

        while let Some(action) = self.relay.pop_local_action() {
            match action {
                LocalRelayAction::Publish(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] on added local channel {} => set Hashmap field", self.node_id, channel);
                    self.outputs.push_back(NetworkBehaviorAction::ToSdkService(
                        KEY_VALUE_SERVICE_ID,
                        KeyValueSdkEvent::SetH(channel as u64, self.node_id as u64, vec![], Some(KEY_VALUE_TIMEOUT_MS)).into(),
                    ));
                }
                LocalRelayAction::Unpublish(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] on removed local channel {} => del Hashmap field", self.node_id, channel);
                    self.outputs.push_back(NetworkBehaviorAction::ToSdkService(
                        KEY_VALUE_SERVICE_ID,
                        KeyValueSdkEvent::DelH(channel as u64, self.node_id as u64).into(),
                    ));
                }
            }
        }

        while let Some(action) = self.relay.pop_source_binding_action() {
            match action {
                SourceBindingAction::Subscribe(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] will sub hashmap {}", self.node_id, channel);
                    self.outputs.push_back(NetworkBehaviorAction::ToSdkService(
                        KEY_VALUE_SERVICE_ID,
                        KeyValueSdkEvent::SubH(KEY_VALUE_SUB_UUID, channel as u64, Some(KEY_VALUE_TIMEOUT_MS)).into(),
                    ));
                }
                SourceBindingAction::Unsubscribe(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] will unsub hashmap {}", self.node_id, channel);
                    self.outputs.push_back(NetworkBehaviorAction::ToSdkService(
                        KEY_VALUE_SERVICE_ID,
                        KeyValueSdkEvent::UnsubH(KEY_VALUE_SUB_UUID, channel as u64).into(),
                    ));
                }
            }
        }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for PubsubServiceBehaviour<BE, HE, SE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
    SE: From<KeyValueSdkEvent> + TryInto<KeyValueSdkEvent>,
{
    fn service_id(&self) -> u8 {
        PUBSUB_SERVICE_ID
    }

    fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        log::info!("[PubSubServiceBehaviour {}] on_started", self.node_id);
        //TODO avoid using awaker in relay, refer sameway with key-value
        self.relay.set_awaker(ctx.awaker.clone());
    }

    fn on_tick(&mut self, ctx: &BehaviorContext, now_ms: u64, _interval_ms: u64) {
        self.relay.tick(now_ms);
        self.pop_all_events(ctx);
    }

    fn on_awake(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        self.pop_all_events(ctx);
    }

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _msg: atm0s_sdn_network::msg::TransportMsg) {}

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId, _local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.relay.on_connection_opened(conn.conn_id(), conn);
        Some(Box::new(PubsubServiceConnectionHandler {
            node_id: self.node_id,
            relay: self.relay.clone(),
        }))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _ctx: &BehaviorContext,
        _now_ms: u64,
        conn: Arc<dyn ConnectionSender>,
        _local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.relay.on_connection_opened(conn.conn_id(), conn);
        Some(Box::new(PubsubServiceConnectionHandler {
            node_id: self.node_id,
            relay: self.relay.clone(),
        }))
    }

    fn on_incoming_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, conn_id: ConnId) {
        self.relay.on_connection_closed(conn_id);
    }

    fn on_outgoing_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, conn_id: ConnId) {
        self.relay.on_connection_closed(conn_id);
    }

    fn on_outgoing_connection_error(
        &mut self,
        _ctx: &BehaviorContext,
        _now_ms: u64,
        _node_id: NodeId,
        conn_id: Option<ConnId>,
        _local_uuid: TransportOutgoingLocalUuid,
        _err: &OutgoingConnectionError,
    ) {
        if let Some(conn_id) = conn_id {
            self.relay.on_connection_closed(conn_id);
        }
    }

    fn on_handler_event(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {
        log::info!("[PubSubServiceBehaviour {}] on_stopped", self.node_id);
    }

    fn on_sdk_msg(&mut self, ctx: &BehaviorContext, _now_ms: u64, from_service: u8, event: SE) {
        if from_service != KEY_VALUE_SERVICE_ID {
            return;
        }

        if let Ok(event) = event.try_into() {
            match event {
                KeyValueSdkEvent::OnKeyHChanged(_uuid, key, _sub_key, value, _version, source) => {
                    if value.is_some() {
                        self.relay.on_source_added(key as u32, source);
                    } else {
                        self.relay.on_source_removed(key as u32, source);
                    }
                    self.pop_all_events(ctx);
                }
                _ => {}
            }
        }
    }

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        self.outputs.pop_front()
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use atm0s_sdn_identity::ConnId;
    use atm0s_sdn_key_value::{KeyValueSdkEvent, KEY_VALUE_SERVICE_ID};
    use atm0s_sdn_network::{
        behaviour::{BehaviorContext, NetworkBehavior, NetworkBehaviorAction},
        msg::{MsgHeader, TransportMsg},
    };
    use atm0s_sdn_router::RouteRule;
    use atm0s_sdn_utils::{awaker::MockAwaker, MockTimer, Timer};

    use crate::{
        behaviour::{KEY_VALUE_SUB_UUID, KEY_VALUE_TIMEOUT_MS},
        handler::CONTROL_META_TYPE,
        ChannelIdentify, PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent, PUBSUB_SERVICE_ID,
    };

    type BE = PubsubServiceBehaviourEvent;
    type HE = PubsubServiceHandlerEvent;
    type SE = KeyValueSdkEvent;

    #[test]
    fn publish_unpublish_should_set_del_key() {
        let local_node_id = 1;
        let channel = 1000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::PubsubServiceBehaviour::<BE, HE, SE>::new(local_node_id, timer.clone());

        let ctx = BehaviorContext {
            service_id: PUBSUB_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        let publisher = sdk.create_publisher(channel);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());
        assert_eq!(
            behaviour.pop_action(),
            Some(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::SetH(channel as u64, local_node_id as u64, vec![], Some(KEY_VALUE_TIMEOUT_MS)).into()
            ))
        );

        drop(publisher);

        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());
        assert_eq!(
            behaviour.pop_action(),
            Some(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::DelH(channel as u64, local_node_id as u64).into()
            ))
        );
    }

    #[test]
    fn sub_unsub_direct_should_send_net() {
        let local_node_id = 1;
        let source_node_id = 10;
        let channel_uuid = 1000;
        let channel = ChannelIdentify::new(channel_uuid, source_node_id);
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::PubsubServiceBehaviour::<BE, HE, SE>::new(local_node_id, timer.clone());

        let ctx = BehaviorContext {
            service_id: PUBSUB_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        let consumer = sdk.create_consumer_single(channel, None);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());
        let expected_header = MsgHeader::build(PUBSUB_SERVICE_ID, PUBSUB_SERVICE_ID, RouteRule::Direct).set_meta(CONTROL_META_TYPE);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &PubsubRemoteEvent::Sub(channel));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNetNode(source_node_id, expected_msg)));

        // after handle ack should not resend
        behaviour
            .relay
            .on_event(timer.now_ms(), source_node_id, ConnId::from_in(0, 0), PubsubRemoteEvent::SubAck(channel, true));

        drop(consumer);

        behaviour.on_awake(&ctx, timer.now_ms());
        let expected_header = MsgHeader::build(PUBSUB_SERVICE_ID, PUBSUB_SERVICE_ID, RouteRule::Direct).set_meta(CONTROL_META_TYPE);
        let expected_msg = TransportMsg::from_payload_bincode(expected_header, &PubsubRemoteEvent::Unsub(channel));
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ToNetConn(ConnId::from_in(0, 0), expected_msg)));
    }

    #[test]
    fn sub_unsub_should_sub_unsub_key() {
        let local_node_id = 1;
        let channel = 1000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::PubsubServiceBehaviour::<BE, HE, SE>::new(local_node_id, timer.clone());

        let ctx = BehaviorContext {
            service_id: PUBSUB_SERVICE_ID,
            node_id: local_node_id,
            awaker: Arc::new(MockAwaker::default()),
        };

        behaviour.on_started(&ctx, 0);

        let consumer = sdk.create_consumer(channel, None);
        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());
        assert_eq!(
            behaviour.pop_action(),
            Some(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::SubH(KEY_VALUE_SUB_UUID, channel as u64, Some(KEY_VALUE_TIMEOUT_MS)).into()
            ))
        );

        drop(consumer);

        assert_eq!(ctx.awaker.pop_awake_count(), 1);

        behaviour.on_awake(&ctx, timer.now_ms());
        assert_eq!(
            behaviour.pop_action(),
            Some(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::UnsubH(KEY_VALUE_SUB_UUID, channel as u64).into()
            ))
        );
    }
}

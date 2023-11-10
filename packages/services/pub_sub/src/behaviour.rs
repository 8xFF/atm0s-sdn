use std::{collections::VecDeque, sync::Arc};

use bluesea_identity::{ConnId, NodeId};
use bluesea_router::RouteRule;
use key_value::{KeyValueSdkEvent, KEY_VALUE_SERVICE_ID};
use network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::{MsgHeader, TransportMsg},
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid},
};
use utils::Timer;

use crate::{
    handler::{PubsubServiceConnectionHandler, CONTROL_META_TYPE, FEEDBACK_TYPE},
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{local::LocalRelayAction, logic::PubsubRelayLogicOutput, source_binding::SourceBindingAction, PubsubRelay},
    PubsubSdk, PUBSUB_SERVICE_ID,
};

pub struct PubsubServiceBehaviour<HE, SE> {
    node_id: NodeId,
    relay: PubsubRelay,
    outputs: VecDeque<NetworkBehaviorAction<HE, SE>>,
}

impl<HE, SE> PubsubServiceBehaviour<HE, SE>
where
    SE: From<KeyValueSdkEvent> + TryInto<KeyValueSdkEvent>,
{
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>) -> (Self, PubsubSdk) {
        let (relay, sdk) = PubsubRelay::new(node_id, timer);
        (
            Self {
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
                    let mut header = MsgHeader::build_reliable(PUBSUB_SERVICE_ID, RouteRule::Direct, 0);
                    header.meta = CONTROL_META_TYPE;
                    TransportMsg::from_payload_bincode(header, &e)
                }
                PubsubRelayLogicOutput::Feedback(fb) => {
                    let mut header = MsgHeader::build_reliable(PUBSUB_SERVICE_ID, RouteRule::Direct, 0);
                    header.meta = FEEDBACK_TYPE;
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
                        KeyValueSdkEvent::SetH(channel as u64, self.node_id as u64, vec![], Some(30000)).into(),
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
                    self.outputs
                        .push_back(NetworkBehaviorAction::ToSdkService(KEY_VALUE_SERVICE_ID, KeyValueSdkEvent::SubH(0, channel as u64, Some(30000)).into()));
                }
                SourceBindingAction::Unsubscribe(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] will unsub hashmap {}", self.node_id, channel);
                    self.outputs
                        .push_back(NetworkBehaviorAction::ToSdkService(KEY_VALUE_SERVICE_ID, KeyValueSdkEvent::UnsubH(0, channel as u64).into()));
                }
            }
        }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for PubsubServiceBehaviour<HE, SE>
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

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _msg: network::msg::TransportMsg) {}

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

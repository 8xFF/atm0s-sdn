use std::{collections::VecDeque, sync::Arc};

use async_std::{channel::Sender, task::JoinHandle};
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::RouteRule;
use network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::{MsgHeader, TransportMsg},
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid},
};
use parking_lot::Mutex;
use utils::Timer;

use crate::{
    handler::{PubsubServiceConnectionHandler, CONTROL_META_TYPE, FEEDBACK_TYPE},
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{local::LocalRelayAction, logic::PubsubRelayLogicOutput, source_binding::SourceBindingAction, PubsubRelay},
    PubsubSdk, PUBSUB_SERVICE_ID,
};

use self::channel_source::{ChannelSourceHashmap, SourceMapEvent};

pub(crate) mod channel_source;

pub struct PubsubServiceBehaviour<HE, SE> {
    node_id: NodeId,
    channel_source_map_tx: Option<Sender<SourceMapEvent>>,
    channel_source_map: Box<dyn ChannelSourceHashmap>,
    relay: PubsubRelay,
    kv_rx_task: Option<JoinHandle<()>>,
    kv_rx_queue: Arc<Mutex<VecDeque<PubsubServiceBehaviourEvent>>>,
    outputs: VecDeque<NetworkBehaviorAction<HE, SE>>,
}

impl<HE, SE> PubsubServiceBehaviour<HE, SE> {
    pub fn new(node_id: NodeId, channel_source_map: Box<dyn ChannelSourceHashmap>, timer: Arc<dyn Timer>) -> (Self, PubsubSdk) {
        let (relay, sdk) = PubsubRelay::new(node_id, timer);
        (
            Self {
                node_id,
                channel_source_map_tx: None,
                channel_source_map,
                relay,
                kv_rx_task: None,
                kv_rx_queue: Arc::new(Mutex::new(VecDeque::new())),
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
                    self.channel_source_map.add(channel as u64);
                }
                LocalRelayAction::Unpublish(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] on removed local channel {} => del Hashmap field", self.node_id, channel);
                    self.channel_source_map.remove(channel as u64);
                }
            }
        }

        while let Some(action) = self.relay.pop_source_binding_action() {
            match action {
                SourceBindingAction::Subscribe(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] will sub hashmap {}", self.node_id, channel);
                    let tx = self.channel_source_map_tx.as_ref().expect("Should has channel_source_map_tx").clone();
                    self.channel_source_map.subscribe(channel as u64, tx);
                }
                SourceBindingAction::Unsubscribe(channel) => {
                    log::info!("[PubSubServiceBehaviour {}] will unsub hashmap {}", self.node_id, channel);
                    self.channel_source_map.unsubscribe(channel as u64);
                }
            }
        }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for PubsubServiceBehaviour<HE, SE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        PUBSUB_SERVICE_ID
    }

    fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        log::info!("[PubSubServiceBehaviour {}] on_started", self.node_id);
        self.relay.set_awaker(ctx.awaker.clone());
        let (tx, rx) = async_std::channel::unbounded();
        self.channel_source_map_tx = Some(tx);
        let node_id = self.node_id;
        let ctx = ctx.clone();
        let queue = self.kv_rx_queue.clone();
        self.kv_rx_task = Some(async_std::task::spawn(async move {
            while let Ok((key, _sub_key, value, _version, source)) = rx.recv().await {
                if value.is_some() {
                    log::debug!("[PubSubServiceBehaviour {}] channel {} add source {}", node_id, key, source);
                    queue.lock().push_back(PubsubServiceBehaviourEvent::OnHashmapSet(key, source))
                } else {
                    log::debug!("[PubSubServiceBehaviour {}] channel {} remove source {}", node_id, key, source);
                    queue.lock().push_back(PubsubServiceBehaviourEvent::OnHashmapDel(key, source))
                }
                ctx.awaker.notify();
            }
        }));
    }

    fn on_tick(&mut self, ctx: &BehaviorContext, now_ms: u64, _interval_ms: u64) {
        self.relay.tick(now_ms);
        self.pop_all_events(ctx);
    }

    fn on_awake(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        loop {
            let event = self.kv_rx_queue.lock().pop_front();
            if let Some(event) = event {
                if let Ok(event) = event.try_into() {
                    match event {
                        PubsubServiceBehaviourEvent::Awake => {
                            self.pop_all_events(ctx);
                        }
                        PubsubServiceBehaviourEvent::OnHashmapSet(channel, source) => {
                            log::info!("[PubSubServiceBehaviour {}] on channel {} added source {}", self.node_id, channel, source);
                            self.relay.on_source_added(channel as u32, source);
                            self.pop_all_events(ctx);
                        }
                        PubsubServiceBehaviourEvent::OnHashmapDel(channel, source) => {
                            log::info!("[PubSubServiceBehaviour {}] on channel {} removed source {}", self.node_id, channel, source);
                            self.relay.on_source_removed(channel as u32, source);
                            self.pop_all_events(ctx);
                        }
                    }
                } else {
                    log::warn!("[PubSubServiceBehaviour {}] invalid event", self.node_id);
                }
            } else {
                break;
            }
        }
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
        if let Some(task) = self.kv_rx_task.take() {
            async_std::task::spawn(async move {
                task.cancel().await;
            });
        }
    }

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        self.outputs.pop_front()
    }
}

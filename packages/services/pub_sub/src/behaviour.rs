use std::sync::Arc;

use async_std::{channel::Sender, task::JoinHandle};
use bluesea_identity::NodeId;
use bluesea_router::RouteRule;
use futures::{select, FutureExt};
use network::{
    behaviour::NetworkBehavior,
    msg::{MsgHeader, TransportMsg},
};
use utils::awaker::{AsyncAwaker, Awaker};

use crate::{
    handler::{PubsubServiceConnectionHandler, CONTROL_META_TYPE, FEEDBACK_TYPE},
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{local::LocalRelayAction, logic::PubsubRelayLogicOutput, source_binding::SourceBindingAction, PubsubRelay},
    PubsubSdk, PUBSUB_SERVICE_ID,
};

use self::channel_source::{ChannelSourceHashmap, SourceMapEvent};

pub(crate) mod channel_source;

pub struct PubsubServiceBehaviour<BE, HE> {
    node_id: NodeId,
    channel_source_map_tx: Option<Sender<SourceMapEvent>>,
    channel_source_map: Box<dyn ChannelSourceHashmap>,
    relay: PubsubRelay<BE, HE>,
    awake_notify: Arc<dyn Awaker>,
    awake_task: Option<JoinHandle<()>>,
}

impl<BE, HE> PubsubServiceBehaviour<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(node_id: NodeId, channel_source_map: Box<dyn ChannelSourceHashmap>) -> (Self, PubsubSdk<BE, HE>) {
        let awake_notify = Arc::new(AsyncAwaker::default());
        let (relay, sdk) = PubsubRelay::new(node_id, awake_notify.clone());
        (
            Self {
                node_id,
                channel_source_map_tx: None,
                channel_source_map,
                relay,
                awake_notify,
                awake_task: None,
            },
            sdk,
        )
    }
}

impl<BE, HE> PubsubServiceBehaviour<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    fn pop_all_events(&mut self, agent: &network::BehaviorAgent<BE, HE>) {
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
                agent.send_to_net_direct(conn, msg);
            } else {
                agent.send_to_net_node(node, msg);
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

impl<BE, HE> NetworkBehavior<BE, HE> for PubsubServiceBehaviour<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        PUBSUB_SERVICE_ID
    }

    fn on_started(&mut self, agent: &network::BehaviorAgent<BE, HE>) {
        log::info!("[PubSubServiceBehaviour {}] on_started", self.node_id);
        let (tx, rx) = async_std::channel::unbounded();
        self.channel_source_map_tx = Some(tx);
        let awake_notify = self.awake_notify.clone();
        let agent = agent.clone();
        let node_id = self.node_id;
        self.awake_task = Some(async_std::task::spawn(async move {
            log::debug!("[PubSubServiceBehaviour {}] start wait loop", node_id);
            loop {
                select! {
                    _ = awake_notify.wait().fuse() => {
                        log::debug!("[PubSubServiceBehaviour {}] awake_notify", node_id);
                        agent.send_to_behaviour(PubsubServiceBehaviourEvent::Awake.into());
                    }
                    e = rx.recv().fuse() => {
                        if let Ok(e) = e {
                            if e.2.is_some() {
                                // log::debug!("[PubSubServiceBehaviour {}] channel {} add source {}", node_id, e.0, e.4);
                                agent.send_to_behaviour(PubsubServiceBehaviourEvent::OnHashmapSet(e.0, e.4).into());
                            } else {
                                // log::debug!("[PubSubServiceBehaviour {}] channel {} remove source {}", node_id, e.0, e.4);
                                agent.send_to_behaviour(PubsubServiceBehaviourEvent::OnHashmapDel(e.0, e.4).into());
                            }
                        }
                    }
                }
            }
        }));
    }

    fn on_tick(&mut self, agent: &network::BehaviorAgent<BE, HE>, _ts_ms: u64, _interval_ms: u64) {
        self.relay.tick();
        self.pop_all_events(agent);
    }

    fn on_local_msg(&mut self, _agent: &network::BehaviorAgent<BE, HE>, _msg: network::msg::TransportMsg) {}

    fn on_local_event(&mut self, agent: &network::BehaviorAgent<BE, HE>, event: BE) {
        if let Ok(event) = event.try_into() {
            match event {
                PubsubServiceBehaviourEvent::Awake => {
                    self.pop_all_events(agent);
                }
                PubsubServiceBehaviourEvent::OnHashmapSet(channel, source) => {
                    log::info!("[PubSubServiceBehaviour {}] on channel {} added source {}", self.node_id, channel, source);
                    self.relay.on_source_added(channel as u32, source);
                    self.pop_all_events(agent);
                }
                PubsubServiceBehaviourEvent::OnHashmapDel(channel, source) => {
                    log::info!("[PubSubServiceBehaviour {}] on channel {} removed source {}", self.node_id, channel, source);
                    self.relay.on_source_removed(channel as u32, source);
                    self.pop_all_events(agent);
                }
            }
        }
    }

    fn check_incoming_connection(&mut self, _node: bluesea_identity::NodeId, _conn_id: bluesea_identity::ConnId) -> Result<(), network::transport::ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _node: bluesea_identity::NodeId, _conn_id: bluesea_identity::ConnId) -> Result<(), network::transport::ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(
        &mut self,
        _agent: &network::BehaviorAgent<BE, HE>,
        _conn: std::sync::Arc<dyn network::transport::ConnectionSender>,
    ) -> Option<Box<dyn network::behaviour::ConnectionHandler<BE, HE>>> {
        Some(Box::new(PubsubServiceConnectionHandler {
            node_id: self.node_id,
            relay: self.relay.clone(),
        }))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _agent: &network::BehaviorAgent<BE, HE>,
        _conn: std::sync::Arc<dyn network::transport::ConnectionSender>,
    ) -> Option<Box<dyn network::behaviour::ConnectionHandler<BE, HE>>> {
        Some(Box::new(PubsubServiceConnectionHandler {
            node_id: self.node_id,
            relay: self.relay.clone(),
        }))
    }

    fn on_incoming_connection_disconnected(&mut self, _agent: &network::BehaviorAgent<BE, HE>, _conn: std::sync::Arc<dyn network::transport::ConnectionSender>) {}

    fn on_outgoing_connection_disconnected(&mut self, _agent: &network::BehaviorAgent<BE, HE>, _conn: std::sync::Arc<dyn network::transport::ConnectionSender>) {}

    fn on_outgoing_connection_error(
        &mut self,
        _agent: &network::BehaviorAgent<BE, HE>,
        _node_id: bluesea_identity::NodeId,
        _conn_id: bluesea_identity::ConnId,
        _err: &network::transport::OutgoingConnectionError,
    ) {
    }

    fn on_handler_event(&mut self, _agent: &network::BehaviorAgent<BE, HE>, _node_id: bluesea_identity::NodeId, _conn_id: bluesea_identity::ConnId, _event: BE) {}

    fn on_stopped(&mut self, _agent: &network::BehaviorAgent<BE, HE>) {
        log::info!("[PubSubServiceBehaviour {}] on_stopped", self.node_id);
        if let Some(task) = self.awake_task.take() {
            async_std::task::spawn(async move {
                task.cancel().await;
            });
        }
    }
}

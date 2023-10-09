use std::sync::Arc;

use async_std::task::JoinHandle;
use bluesea_identity::NodeId;
use bluesea_router::RouteRule;
use network::{
    behaviour::NetworkBehavior,
    msg::{MsgHeader, TransportMsg},
};
use utils::awaker::{AsyncAwaker, Awaker};

use crate::{
    handler::{PubsubServiceConnectionHandler, CONTROL_META_TYPE, FEEDBACK_TYPE},
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{logic::PubsubRelayLogicOutput, PubsubRelay},
    PubsubSdk, PUBSUB_SERVICE_ID,
};

pub struct PubsubServiceBehaviour<BE, HE> {
    relay: PubsubRelay<BE, HE>,
    awake_notify: Arc<dyn Awaker>,
    awake_task: Option<JoinHandle<()>>,
}

impl<BE, HE> PubsubServiceBehaviour<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(node_id: NodeId) -> (Self, PubsubSdk<BE, HE>) {
        let awake_notify = Arc::new(AsyncAwaker::default());
        let (relay, sdk) = PubsubRelay::new(node_id, awake_notify.clone());
        (
            Self {
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
        while let Some((node, conn, action)) = self.relay.pop_action() {
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
        log::info!("[PubSubServiceBehaviour] on_started");
        let awake_notify = self.awake_notify.clone();
        let agent = agent.clone();
        self.awake_task = Some(async_std::task::spawn(async move {
            loop {
                awake_notify.wait().await;
                log::debug!("[KeyValueBehavior] awake_notify");
                agent.send_to_behaviour(PubsubServiceBehaviourEvent::Awake.into());
            }
        }));
    }

    fn on_tick(&mut self, agent: &network::BehaviorAgent<BE, HE>, _ts_ms: u64, _interval_ms: u64) {
        self.relay.tick();
        self.pop_all_events(agent);
    }

    fn on_local_msg(&mut self, _agent: &network::BehaviorAgent<BE, HE>, _msg: network::msg::TransportMsg) {}

    fn on_local_event(&mut self, agent: &network::BehaviorAgent<BE, HE>, _event: BE) {
        self.pop_all_events(agent);
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
        Some(Box::new(PubsubServiceConnectionHandler { relay: self.relay.clone() }))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _agent: &network::BehaviorAgent<BE, HE>,
        _conn: std::sync::Arc<dyn network::transport::ConnectionSender>,
    ) -> Option<Box<dyn network::behaviour::ConnectionHandler<BE, HE>>> {
        Some(Box::new(PubsubServiceConnectionHandler { relay: self.relay.clone() }))
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
        log::info!("[PubSubServiceBehaviour] on_stopped");
        if let Some(task) = self.awake_task.take() {
            async_std::task::spawn(async move {
                task.cancel().await;
            });
        }
    }
}

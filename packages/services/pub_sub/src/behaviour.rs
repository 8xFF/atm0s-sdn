use bluesea_identity::NodeId;
use bluesea_router::RouteRule;
use network::{
    behaviour::NetworkBehavior,
    msg::{MsgHeader, TransportMsg},
};

use crate::{
    handler::{PubsubServiceConnectionHandler, CONTROL_META_TYPE},
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::PubsubRelay,
    PubsubSdk, PUBSUB_SERVICE_ID,
};

pub struct PubSubServiceBehaviour<BE, HE> {
    relay: PubsubRelay<BE, HE>,
}

impl<BE, HE> PubSubServiceBehaviour<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(node_id: NodeId) -> (Self, PubsubSdk<BE, HE>) {
        let (relay, sdk) = PubsubRelay::new(node_id);
        (Self { relay }, sdk)
    }
}

impl<BE, HE> PubSubServiceBehaviour<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    fn pop_all_events(&mut self, agent: &network::BehaviorAgent<BE, HE>) {
        while let Some((node, conn, action)) = self.relay.pop_action() {
            let mut header = MsgHeader::build_reliable(PUBSUB_SERVICE_ID, RouteRule::Direct, 0);
            header.meta = CONTROL_META_TYPE;
            let msg = TransportMsg::from_payload_bincode(header, &action);

            //Should be send to correct conn, if that conn not exits => fallback by finding to origin source node
            if let Some(conn) = conn {
                agent.send_to_net_direct(conn, msg);
            } else {
                agent.send_to_net_node(node, msg);
            }
        }
    }
}

impl<BE, HE> NetworkBehavior<BE, HE> for PubSubServiceBehaviour<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        PUBSUB_SERVICE_ID
    }

    fn on_started(&mut self, agent: &network::BehaviorAgent<BE, HE>) {}

    fn on_tick(&mut self, agent: &network::BehaviorAgent<BE, HE>, _ts_ms: u64, _interval_ms: u64) {
        self.relay.tick();
        self.pop_all_events(agent);
    }

    fn on_local_msg(&mut self, agent: &network::BehaviorAgent<BE, HE>, msg: network::msg::TransportMsg) {}

    fn on_local_event(&mut self, agent: &network::BehaviorAgent<BE, HE>, event: BE) {
        self.pop_all_events(agent);
    }

    fn check_incoming_connection(&mut self, node: bluesea_identity::NodeId, conn_id: bluesea_identity::ConnId) -> Result<(), network::transport::ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, node: bluesea_identity::NodeId, conn_id: bluesea_identity::ConnId) -> Result<(), network::transport::ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(
        &mut self,
        agent: &network::BehaviorAgent<BE, HE>,
        conn: std::sync::Arc<dyn network::transport::ConnectionSender>,
    ) -> Option<Box<dyn network::behaviour::ConnectionHandler<BE, HE>>> {
        Some(Box::new(PubsubServiceConnectionHandler { relay: self.relay.clone() }))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        agent: &network::BehaviorAgent<BE, HE>,
        conn: std::sync::Arc<dyn network::transport::ConnectionSender>,
    ) -> Option<Box<dyn network::behaviour::ConnectionHandler<BE, HE>>> {
        Some(Box::new(PubsubServiceConnectionHandler { relay: self.relay.clone() }))
    }

    fn on_incoming_connection_disconnected(&mut self, agent: &network::BehaviorAgent<BE, HE>, conn: std::sync::Arc<dyn network::transport::ConnectionSender>) {}

    fn on_outgoing_connection_disconnected(&mut self, agent: &network::BehaviorAgent<BE, HE>, conn: std::sync::Arc<dyn network::transport::ConnectionSender>) {}

    fn on_outgoing_connection_error(
        &mut self,
        agent: &network::BehaviorAgent<BE, HE>,
        node_id: bluesea_identity::NodeId,
        conn_id: bluesea_identity::ConnId,
        err: &network::transport::OutgoingConnectionError,
    ) { }

    fn on_handler_event(&mut self, agent: &network::BehaviorAgent<BE, HE>, node_id: bluesea_identity::NodeId, conn_id: bluesea_identity::ConnId, event: BE) {}

    fn on_stopped(&mut self, agent: &network::BehaviorAgent<BE, HE>) {}
}

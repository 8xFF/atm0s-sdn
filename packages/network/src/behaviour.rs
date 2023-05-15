use crate::transport::{ConnectionEvent, ConnectionSender, OutgoingConnectionError};
use std::sync::Arc;
use bluesea_identity::PeerId;
use crate::plane::{ConnectionAgent, NetworkAgent};

pub trait ConnectionHandler<BE>: Send + Sync {
    fn on_opened(&mut self, agent: &ConnectionAgent<BE>);
    fn on_tick(&mut self, agent: &ConnectionAgent<BE>, ts_ms: u64, interal_ms: u64);
    fn on_event(&mut self, agent: &ConnectionAgent<BE>, event: &ConnectionEvent);
    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE>, event: BE);
    fn on_closed(&mut self, agent: &ConnectionAgent<BE>);
}

pub enum NetworkBehaviorEvent {}

pub trait NetworkBehavior<BE, HE> {
    fn on_tick(&mut self, agent: &NetworkAgent<HE>, ts_ms: u64, interal_ms: u64);
    fn on_incoming_connection_connected(
        &mut self,
        agent: &NetworkAgent<HE>,
        connection: Arc<dyn ConnectionSender>,
    ) -> Option<Box<dyn ConnectionHandler<BE>>>;
    fn on_outgoing_connection_connected(
        &mut self,
        agent: &NetworkAgent<HE>,
        connection: Arc<dyn ConnectionSender>,
    ) -> Option<Box<dyn ConnectionHandler<BE>>>;
    fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError);
    fn on_event(&mut self, agent: &NetworkAgent<HE>, event: NetworkBehaviorEvent);
    fn on_handler_event(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, event: HE);
}

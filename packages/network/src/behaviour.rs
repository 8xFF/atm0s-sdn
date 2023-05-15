use crate::transport::{ConnectionEvent, ConnectionSender, OutgoingConnectionError};
use std::sync::Arc;
use crate::plane::NetworkAgent;

pub trait ConnectionHandler: Send + Sync {
    fn on_opened(&mut self, agent: &NetworkAgent);
    fn on_tick(&mut self, agent: &NetworkAgent, ts_ms: u64, interal_ms: u64);
    fn on_event(&mut self, agent: &NetworkAgent, event: &ConnectionEvent);
    fn on_closed(&mut self, agent: &NetworkAgent);
}

pub enum NetworkBehaviorEvent {}

pub trait NetworkBehavior {
    fn on_tick(&mut self, agent: &NetworkAgent, ts_ms: u64, interal_ms: u64);
    fn on_incoming_connection_connected(
        &mut self,
        agent: &NetworkAgent,
        connection: Arc<dyn ConnectionSender>,
    ) -> Option<Box<dyn ConnectionHandler>>;
    fn on_outgoing_connection_connected(
        &mut self,
        agent: &NetworkAgent,
        connection: Arc<dyn ConnectionSender>,
    ) -> Option<Box<dyn ConnectionHandler>>;
    fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent, connection_id: u32, err: &OutgoingConnectionError);
    fn on_event(&mut self, agent: &NetworkAgent, event: NetworkBehaviorEvent);
}

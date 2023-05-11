use crate::transport::{ConnectionEvent, ConnectionSender, OutgoingConnectionError};
use std::sync::Arc;

pub trait ConnectionHandler {
    fn on_event(&mut self, event: &ConnectionEvent);
}

pub enum NetworkBehaviorEvent {}

pub trait NetworkBehavior {
    fn on_incoming_connection_connected(
        &mut self,
        connection: Arc<dyn ConnectionSender>,
    ) -> Option<Box<dyn ConnectionHandler>>;
    fn on_outgoing_connection_connected(
        &mut self,
        connection: Arc<dyn ConnectionSender>,
    ) -> Option<Box<dyn ConnectionHandler>>;
    fn on_incoming_connection_disconnected(&mut self, connection: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_disconnected(&mut self, connection: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_error(&mut self, connection_id: u32, err: &OutgoingConnectionError);
    fn on_event(&mut self, event: NetworkBehaviorEvent);
}

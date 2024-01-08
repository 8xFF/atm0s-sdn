use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction},
    transport::ConnectionEvent,
};

use crate::vnet::internal::VirtualNetInternal;

pub struct VirtualSocketHandler {
    pub(crate) internal: VirtualNetInternal,
}

impl<BE, HE> ConnectionHandler<BE, HE> for VirtualSocketHandler {
    /// Called when the connection is opened.
    fn on_opened(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    /// Called on each tick of the connection.
    fn on_tick(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _interval_ms: u64) {}

    /// Called when the connection is awake.
    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    /// Called when an event occurs on the connection.
    fn on_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, event: ConnectionEvent) {
        if let ConnectionEvent::Msg(msg) = event {
            self.internal.on_incomming(msg);
        }
    }

    /// Called when an event occurs on another handler.
    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    /// Called when an event occurs on the behavior.
    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _event: HE) {}

    /// Called when the connection is closed.
    fn on_closed(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    /// Pops the next action to be taken by the connection handler.
    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>> {
        None
    }
}

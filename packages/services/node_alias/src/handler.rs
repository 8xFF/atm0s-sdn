use std::sync::Arc;

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction},
    msg::{MsgHeader, TransportMsg},
    transport::ConnectionEvent,
};
use atm0s_sdn_pub_sub::Publisher;
use atm0s_sdn_router::RouteRule;
use bytes::Bytes;
use parking_lot::Mutex;

use crate::{
    internal::{ServiceInternal, ServiceInternalAction},
    msg::DirectMsg,
    NODE_ALIAS_SERVICE_ID,
};

pub struct NodeAliasHandler {
    pub(crate) node_id: NodeId,
    pub(crate) pub_channel: Arc<Publisher>,
    pub(crate) internal: Arc<Mutex<ServiceInternal>>,
}

impl<BE, HE> ConnectionHandler<BE, HE> for NodeAliasHandler {
    /// Called when the connection is opened.
    fn on_opened(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    /// Called on each tick of the connection.
    fn on_tick(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _interval_ms: u64) {}

    /// Called when the connection is awake.
    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    /// Called when an event occurs on the connection.
    fn on_event(&mut self, _ctx: &ConnectionContext, now_ms: u64, event: ConnectionEvent) {
        if let ConnectionEvent::Msg(msg) = event {
            if let Some(from) = msg.header.from_node {
                if let Ok(msg) = msg.get_payload_bincode::<DirectMsg>() {
                    self.internal.lock().on_incomming_unicast(now_ms, from, msg);
                }
            }
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
        match self.internal.lock().pop_action() {
            Some(ServiceInternalAction::Broadcast(msg)) => {
                log::info!("[NodeAliasHandler {}] Broadcasting: {:?}", self.node_id, msg);
                let msg = bincode::serialize(&msg).unwrap();
                self.pub_channel.send(Bytes::from(msg));
                None
            }
            Some(ServiceInternalAction::Unicast(dest, msg)) => {
                log::info!("[NodeAliasHandler {}] Unicasting to {}: {:?}", self.node_id, dest, msg);
                let header = MsgHeader::build(NODE_ALIAS_SERVICE_ID, NODE_ALIAS_SERVICE_ID, RouteRule::ToNode(dest)).set_from_node(Some(self.node_id));
                Some(ConnectionHandlerAction::ToNet(TransportMsg::from_payload_bincode(header, &msg)))
            }
            None => None,
        }
    }
}

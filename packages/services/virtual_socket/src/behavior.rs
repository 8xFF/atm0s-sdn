use std::sync::Arc;

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::TransportMsg,
    transport::{ConnectionEvent, ConnectionRejectReason, ConnectionSender, OutgoingConnectionError},
};
use parking_lot::RwLock;

use crate::{
    handler::VirtualSocketHandler,
    state::{process_incoming_data, State},
    VirtualSocketSdk, VIRTUAL_SOCKET_SERVICE_ID,
};

pub struct VirtualSocketBehavior {
    node_id: NodeId,
    state: Arc<RwLock<State>>,
}

impl VirtualSocketBehavior {
    pub fn new(node_id: NodeId) -> (Self, VirtualSocketSdk) {
        log::info!("[VirtualSocketBehavior] create new on node: {}", node_id);
        let state = Arc::new(RwLock::new(State::default()));
        (Self { node_id, state: state.clone() }, VirtualSocketSdk::new(state))
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for VirtualSocketBehavior {
    fn service_id(&self) -> u8 {
        VIRTUAL_SOCKET_SERVICE_ID
    }

    fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        self.state.write().set_awaker(ctx.awaker.clone());
    }

    fn on_tick(&mut self, _ctx: &BehaviorContext, now_ms: u64, _interval_ms: u64) {
        self.state.write().on_tick(now_ms);
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, now_ms: u64, msg: TransportMsg) {
        process_incoming_data(now_ms, &self.state, ConnectionEvent::Msg(msg));
    }

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(VirtualSocketHandler { state: self.state.clone() }))
    }

    fn on_outgoing_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(VirtualSocketHandler { state: self.state.clone() }))
    }

    fn on_incoming_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_error(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        let (socket_id, out) = self.state.write().pop_outgoing()?;
        let msg = out.into_transport_msg(self.node_id, socket_id.node_id(), socket_id.client_id());
        Some(NetworkBehaviorAction::ToNet(msg))
    }
}

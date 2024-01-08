use std::sync::Arc;

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::TransportMsg,
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError},
};

use crate::{handler::VirtualSocketHandler, vnet::internal::VirtualNetInternal, VIRTUAL_SOCKET_SERVICE_ID};

pub struct VirtualSocketBehavior {
    internal: VirtualNetInternal,
}

impl VirtualSocketBehavior {
    pub fn new(internal: VirtualNetInternal) -> Self {
        Self { internal }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for VirtualSocketBehavior {
    fn service_id(&self) -> u8 {
        VIRTUAL_SOCKET_SERVICE_ID
    }

    fn on_started(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn on_tick(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _interval_ms: u64) {}

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _msg: TransportMsg) {
        panic!("Should not happend");
    }

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.internal.add_conn(conn);
        Some(Box::new(VirtualSocketHandler { internal: self.internal.clone() }))
    }

    fn on_outgoing_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.internal.add_conn(conn);
        Some(Box::new(VirtualSocketHandler { internal: self.internal.clone() }))
    }

    fn on_incoming_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, conn_id: ConnId) {
        self.internal.remove_conn(conn_id);
    }

    fn on_outgoing_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, conn_id: ConnId) {
        self.internal.remove_conn(conn_id);
    }

    fn on_outgoing_connection_error(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, conn_id: ConnId, _err: &OutgoingConnectionError) {
        self.internal.remove_conn(conn_id);
    }

    fn on_handler_event(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        None
    }
}

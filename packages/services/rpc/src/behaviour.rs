use std::sync::Arc;

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::TransportMsg,
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid},
};

use crate::{handler::RpcHandler, RpcBoxBehaviour};

pub struct RpcBehavior {
    logic: RpcBoxBehaviour,
    service_id: u8,
}

impl RpcBehavior {
    pub fn new(service_id: u8, logic: RpcBoxBehaviour) -> Self {
        Self { service_id, logic }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for RpcBehavior {
    fn service_id(&self) -> u8 {
        self.service_id
    }

    fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        self.logic.set_awaker(ctx.awaker.clone());
    }

    fn on_tick(&mut self, _ctx: &BehaviorContext, now_ms: u64, _interval_ms: u64) {
        self.logic.on_tick(now_ms);
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, now_ms: u64) {
        self.logic.on_tick(now_ms);
    }

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, now_ms: u64, msg: TransportMsg) {
        self.logic.on_msg(now_ms, msg);
    }

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId, _local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(RpcHandler { logic: self.logic.create_handler() }))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _ctx: &BehaviorContext,
        _now_ms: u64,
        _conn: Arc<dyn ConnectionSender>,
        _local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(RpcHandler { logic: self.logic.create_handler() }))
    }

    fn on_incoming_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_error(
        &mut self,
        _ctx: &BehaviorContext,
        _now_ms: u64,
        _node_id: NodeId,
        _conn_id: Option<ConnId>,
        _local_uuid: TransportOutgoingLocalUuid,
        _err: &OutgoingConnectionError,
    ) {
    }

    fn on_handler_event(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        self.logic.pop_net_out().map(|msg| NetworkBehaviorAction::ToNet(msg))
    }
}

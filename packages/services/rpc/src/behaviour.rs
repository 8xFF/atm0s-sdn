use std::sync::Arc;

use async_std::channel::Sender;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::TransportMsg,
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid},
};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use parking_lot::Mutex;

use crate::{
    handler::RpcHandler,
    rpc_msg::{RpcError, RpcMsg},
    rpc_queue::RpcQueue,
};

pub struct RpcBehavior {
    pub(crate) rpc_queue: Arc<Mutex<RpcQueue<Sender<Result<RpcMsg, RpcError>>>>>,
    pub(crate) service_id: u8,
    pub(crate) tx: Sender<RpcMsg>,
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for RpcBehavior {
    fn service_id(&self) -> u8 {
        self.service_id
    }

    fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        self.rpc_queue.lock().set_awaker(ctx.awaker.clone());
    }

    fn on_tick(&mut self, _ctx: &BehaviorContext, now_ms: u64, _interval_ms: u64) {
        while let Some((_req_id, tx)) = self.rpc_queue.lock().pop_timeout(now_ms) {
            tx.try_send(Err(RpcError::Timeout)).print_error("Should send");
        }
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, msg: TransportMsg) {
        if let Ok(msg) = RpcMsg::try_from(&msg) {
            if msg.is_answer() {
                let req_id = msg.req_id().expect("Should has");
                if let Some(tx) = self.rpc_queue.lock().take_request(req_id) {
                    tx.try_send(Ok(msg)).print_error("Should send");
                }
            } else {
                self.tx.try_send(msg).print_error("Should send");
            }
        }
    }

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId, _local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(RpcHandler {
            rpc_queue: self.rpc_queue.clone(),
            tx: self.tx.clone(),
        }))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _ctx: &BehaviorContext,
        _now_ms: u64,
        _conn: Arc<dyn ConnectionSender>,
        _local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(RpcHandler {
            rpc_queue: self.rpc_queue.clone(),
            tx: self.tx.clone(),
        }))
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
        self.rpc_queue.lock().pop_transmit().map(|msg| NetworkBehaviorAction::ToNet(msg))
    }
}

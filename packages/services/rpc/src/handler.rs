use std::sync::Arc;

use async_std::channel::Sender;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction},
    transport::ConnectionEvent,
};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use parking_lot::Mutex;

use crate::{
    rpc_msg::{RpcError, RpcMsg},
    rpc_queue::RpcQueue,
};

pub struct RpcHandler {
    pub(crate) rpc_queue: Arc<Mutex<RpcQueue<Sender<Result<RpcMsg, RpcError>>>>>,
    pub(crate) tx: Sender<RpcMsg>,
}

impl<BE, HE> ConnectionHandler<BE, HE> for RpcHandler {
    fn on_opened(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_tick(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _interval_ms: u64) {}

    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, event: ConnectionEvent) {
        if let ConnectionEvent::Msg(msg) = event {
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
    }

    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _event: HE) {}

    fn on_closed(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>> {
        None
    }
}

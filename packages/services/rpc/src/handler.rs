use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction},
    transport::ConnectionEvent,
};

use crate::RpcBoxHandler;

pub struct RpcHandler {
    pub(crate) logic: RpcBoxHandler,
}

impl<BE, HE> ConnectionHandler<BE, HE> for RpcHandler {
    fn on_opened(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_tick(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _interval_ms: u64) {}

    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_event(&mut self, _ctx: &ConnectionContext, now_ms: u64, event: ConnectionEvent) {
        if let ConnectionEvent::Msg(msg) = event {
            self.logic.on_msg(now_ms, msg);
        }
    }

    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _event: HE) {}

    fn on_closed(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>> {
        None
    }
}

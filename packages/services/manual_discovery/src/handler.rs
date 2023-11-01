use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{ConnectionContext, ConnectionHandler};
use network::transport::ConnectionEvent;

pub struct ManualHandler {}

impl<BE, HE> ConnectionHandler<BE, HE> for ManualHandler
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    fn on_opened(&mut self, _ctx: &ConnectionContext, now_ms: u64) {}

    fn on_tick(&mut self, _ctx: &ConnectionContext, now_ms: u64, _interal_ms: u64) {}

    fn on_awake(&mut self, ctx: &ConnectionContext, now_ms: u64) {}

    fn on_event(&mut self, _ctx: &ConnectionContext, now_ms: u64, _event: ConnectionEvent) {}

    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, now_ms: u64, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, now_ms: u64, _event: HE) {}

    fn on_closed(&mut self, _ctx: &ConnectionContext, now_ms: u64) {}

    fn pop_action(&mut self) -> Option<network::behaviour::ConnectionHandlerAction<BE, HE>> {
        None
    }
}

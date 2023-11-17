use async_std::channel::Sender;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction};
use atm0s_sdn_network::msg::TransportMsg;
use atm0s_sdn_network::transport::ConnectionEvent;
use atm0s_sdn_utils::error_handle::ErrorUtils;

pub struct TunTapHandler {
    pub(crate) local_tx: Sender<TransportMsg>,
}

impl<BE, HE> ConnectionHandler<BE, HE> for TunTapHandler
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    fn on_opened(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_tick(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _interval_ms: u64) {}

    fn on_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, event: ConnectionEvent) {
        if let ConnectionEvent::Msg(msg) = event {
            self.local_tx.try_send(msg).print_error("Should send to local");
        }
    }

    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _event: HE) {}

    fn on_closed(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>> {
        None
    }
}

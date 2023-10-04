use async_std::channel::Sender;
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::ConnectionHandler;
use network::msg::TransportMsg;
use network::transport::ConnectionEvent;
use network::ConnectionAgent;
use utils::error_handle::ErrorUtils;

pub struct TunTapHandler {
    pub(crate) local_tx: Sender<TransportMsg>,
}

impl<BE, HE> ConnectionHandler<BE, HE> for TunTapHandler
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    fn on_opened(&mut self, _agent: &ConnectionAgent<BE, HE>) {}

    fn on_tick(&mut self, _agent: &ConnectionAgent<BE, HE>, _ts_ms: u64, _interal_ms: u64) {}

    fn on_event(&mut self, _agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {
        if let ConnectionEvent::Msg(msg) = event {
            self.local_tx.try_send(msg).print_error("Should send to local");
        }
    }

    fn on_other_handler_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _event: HE) {}

    fn on_closed(&mut self, _agent: &ConnectionAgent<BE, HE>) {}
}

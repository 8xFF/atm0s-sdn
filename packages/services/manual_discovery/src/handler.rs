use bluesea_identity::{ConnId, NodeId};
use network::behaviour::ConnectionHandler;
use network::transport::ConnectionEvent;
use network::ConnectionAgent;

pub struct ManualHandler {}

impl<BE, HE> ConnectionHandler<BE, HE> for ManualHandler
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE>) {}

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE>, ts_ms: u64, interal_ms: u64) {}

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {}

    fn on_other_handler_event(&mut self, agent: &ConnectionAgent<BE, HE>, from_node: NodeId, from_conn: ConnId, event: HE) {}

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: HE) {}

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE>) {}
}

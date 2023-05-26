use bluesea_identity::PeerId;
use network::behaviour::ConnectionHandler;
use network::transport::ConnectionEvent;
use network::ConnectionAgent;

pub struct ManualHandler {}

impl<BE, HE, MSG> ConnectionHandler<BE, HE, MSG> for ManualHandler
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
    MSG: Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {}

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64) {}

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>) {}

    fn on_other_handler_event(
        &mut self,
        agent: &ConnectionAgent<BE, HE, MSG>,
        from_peer: PeerId,
        from_conn: u32,
        event: HE,
    ) {
    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE) {}

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {}
}

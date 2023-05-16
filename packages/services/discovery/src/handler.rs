use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg};
use bluesea_identity::PeerId;
use network::behaviour::ConnectionHandler;
use network::plane::{BehaviorAgent, ConnectionAgent};
use network::transport::ConnectionEvent;

pub struct DiscoveryConnectionHandler {}

impl DiscoveryConnectionHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl<BE, HE, MSG> ConnectionHandler<BE, HE, MSG> for DiscoveryConnectionHandler
where
    BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent>,
    HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent>,
    MSG: TryInto<DiscoveryMsg> + From<DiscoveryMsg>,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {
        todo!()
    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64) {
        todo!()
    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>) {
        todo!()
    }

    fn on_other_handler_event(
        &mut self,
        agent: &ConnectionAgent<BE, HE, MSG>,
        from_peer: PeerId,
        from_conn: u32,
        event: HE,
    ) {
        todo!()
    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE) {
        todo!()
    }

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {
        todo!()
    }
}

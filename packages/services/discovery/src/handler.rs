use network::behaviour::ConnectionHandler;
use network::plane::{ConnectionAgent, BehaviorAgent};
use network::transport::ConnectionEvent;
use crate::msg::{DiscoveryBehaviorEvent, DiscoveryMsg};

pub struct DiscoveryConnectionHandler {

}

impl DiscoveryConnectionHandler {
    pub fn new() -> Self {
        Self {

        }
    }
}

impl<BE, MSG> ConnectionHandler<BE, MSG> for DiscoveryConnectionHandler
    where BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent>,
          MSG: TryInto<DiscoveryMsg> + From<DiscoveryMsg>,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, MSG>) {
        todo!()
    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, MSG>, ts_ms: u64, interal_ms: u64) {
        todo!()
    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: ConnectionEvent<MSG>) {
        todo!()
    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: BE) {
        todo!()
    }

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, MSG>) {
        todo!()
    }
}
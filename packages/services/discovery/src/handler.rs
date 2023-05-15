use network::behaviour::ConnectionHandler;
use network::plane::{ConnectionAgent, NetworkAgent};
use network::transport::ConnectionEvent;
use crate::msg::DiscoveryBehaviorEvent;

pub struct DiscoveryConnectionHandler {

}

impl DiscoveryConnectionHandler {
    pub fn new() -> Self {
        Self {

        }
    }
}

impl<BE> ConnectionHandler<BE> for DiscoveryConnectionHandler
    where BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent>,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE>) {
        todo!()
    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE>, ts_ms: u64, interal_ms: u64) {
        todo!()
    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE>, event: &ConnectionEvent) {
        match event {
            ConnectionEvent::Reliable { stream_id, data } => {

            }
            ConnectionEvent::Unreliable { .. } => {}
            ConnectionEvent::Stats { .. } => {}
        }
    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE>, event: BE) {
        todo!()
    }

    fn on_closed(&mut self, agent: &ConnectionAgent<BE>) {
        todo!()
    }
}
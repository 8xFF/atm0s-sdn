use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent};
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::ConnectionHandler;
use network::transport::ConnectionEvent;
use network::ConnectionAgent;

pub struct DiscoveryConnectionHandler {}

impl DiscoveryConnectionHandler {
    pub fn new() -> Self {
        Self {}
    }
}

#[allow(unused_variables)]
impl<BE, HE> ConnectionHandler<BE, HE> for DiscoveryConnectionHandler
where
    BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent> + Send + Sync + 'static,
    HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE>) {}

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE>, ts_ms: u64, interal_ms: u64) {}

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => {
                // if let Ok(msg) = msg..try_into() {
                //     agent.send_behavior(DiscoveryBehaviorEvent::OnNetworkMessage(msg).into());
                // }
            }
            ConnectionEvent::Stats(stats) => {}
        }
    }

    fn on_other_handler_event(&mut self, agent: &ConnectionAgent<BE, HE>, from_node: NodeId, from_conn: ConnId, event: HE) {}

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: HE) {}

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE>) {}
}

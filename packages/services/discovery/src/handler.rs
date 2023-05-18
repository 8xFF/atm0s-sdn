use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg};
use bluesea_identity::PeerId;
use network::behaviour::ConnectionHandler;
use network::plane::{BehaviorAgent, ConnectionAgent};
use network::transport::{ConnectionEvent, ConnectionMsg};

pub struct DiscoveryConnectionHandler {}

impl DiscoveryConnectionHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl<BE, HE, MSG> ConnectionHandler<BE, HE, MSG> for DiscoveryConnectionHandler
where
    BE: TryInto<DiscoveryBehaviorEvent<MSG>> + From<DiscoveryBehaviorEvent<MSG>> + Send + Sync + 'static,
    HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent> + Send + Sync + 'static,
    MSG: TryInto<DiscoveryMsg> + From<DiscoveryMsg> + Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {

    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64) {

    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>) {
        match event {
            ConnectionEvent::Msg { msg, .. } => {
                match msg {
                    ConnectionMsg::Reliable { data, .. } => {
                        agent.send_behavior(DiscoveryBehaviorEvent::OnNetworkMessage(data).into());
                    }
                    _ => {}
                }
            },
            ConnectionEvent::Stats(stats) => {}
        }
    }

    fn on_other_handler_event(
        &mut self,
        agent: &ConnectionAgent<BE, HE, MSG>,
        from_peer: PeerId,
        from_conn: u32,
        event: HE,
    ) {

    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE) {

    }

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {

    }
}

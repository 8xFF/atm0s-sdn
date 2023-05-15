use network::behaviour::ConnectionHandler;
use network::plane::NetworkAgent;
use network::transport::ConnectionEvent;

pub struct DiscoveryConnectionHandler {

}

impl DiscoveryConnectionHandler {
    pub fn new() -> Self {
        Self {

        }
    }
}

impl ConnectionHandler for DiscoveryConnectionHandler {
    fn on_opened(&mut self, agent: &NetworkAgent) {
        todo!()
    }

    fn on_tick(&mut self, agent: &NetworkAgent, ts_ms: u64, interal_ms: u64) {
        todo!()
    }

    fn on_event(&mut self, agent: &NetworkAgent, event: &ConnectionEvent) {
        // match event {
        //     ConnectionEvent::Reliable { stream_id, data } => {
        //         //check if is FIND_NODE_RESPONSE
        //         let closest_peer_ids = vec![];
        //         for id in closest_peer_ids {
        //             agent.connect_to(id);
        //         }
        //     }
        //     ConnectionEvent::Unreliable { .. } => {}
        //     ConnectionEvent::Stats { .. } => {}
        // }
    }

    fn on_closed(&mut self, agent: &NetworkAgent) {
        todo!()
    }
}
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use bluesea_identity::PeerId;
use network::behaviour::{ConnectionHandler, NetworkBehavior, NetworkBehaviorEvent};
use network::plane::NetworkAgent;
use network::transport::{ConnectionSender, OutgoingConnectionError, TransportAddr, TransportPendingOutgoing};
use utils::Timer;
use crate::logic::DiscoveryLogic;

pub struct DiscoveryNetworkBehaviorOpts {
    pub local_node_id: PeerId,
    pub bootstrap_addrs: Option<Vec<TransportAddr>>,
    pub timer: Box<dyn Timer>,
}

pub struct DiscoveryNetworkBehavior {
    logic: Arc<Mutex<DiscoveryLogic>>,
    opts: DiscoveryNetworkBehaviorOpts,
}

impl DiscoveryNetworkBehavior {
    pub fn new(opts: DiscoveryNetworkBehaviorOpts) -> Self {
        todo!()
        // Self {
        //     logic: Arc::new(Mutex::new(DiscoveryLogic::new())),
        //     opts
        // }
    }
}

impl NetworkBehavior for DiscoveryNetworkBehavior {
    fn on_tick(&mut self, agent: &NetworkAgent, ts_ms: u64, interal_ms: u64) {
        todo!()
        // if let Some(bootstrap) = self.opts.bootstrap_addrs.take() {
        //     for addr in bootstrap {
        //         match agent.connect_to(addr.clone()) {
        //             Ok(conn) => {
        //                 log::info!("Connecting to {:?} with connection_id {}", addr, conn.connection_id);
        //             }
        //             Err(err) => {
        //                 log::error!("Connect to {:?} error {:?}", addr, err);
        //             }
        //         }
        //     }
        // }
    }

    fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler>> {
        todo!()
    }

    fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler>> {
        todo!()
    }

    fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) {
        todo!()
    }

    fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) {
        todo!()
    }

    fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent, connection_id: u32, err: &OutgoingConnectionError) {
        todo!()
    }

    fn on_event(&mut self, agent: &NetworkAgent, event: NetworkBehaviorEvent) {
        todo!()
    }
}
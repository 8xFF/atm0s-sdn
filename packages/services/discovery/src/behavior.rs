use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use bluesea_identity::{PeerAddr, PeerId};
use network::behaviour::{ConnectionHandler, NetworkBehavior, NetworkBehaviorEvent};
use network::plane::NetworkAgent;
use network::transport::{ConnectionSender, OutgoingConnectionError, TransportPendingOutgoing};
use utils::Timer;
use crate::handler::DiscoveryConnectionHandler;
use crate::logic::{Action, DiscoveryLogic, DiscoveryLogicConf, Input};
use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent};

pub struct DiscoveryNetworkBehaviorOpts {
    pub local_node_id: PeerId,
    pub bootstrap_addrs: Option<Vec<(PeerId, PeerAddr)>>,
    pub timer: Arc<dyn Timer>,
}

pub struct DiscoveryNetworkBehavior {
    logic: Arc<Mutex<DiscoveryLogic>>,
    opts: DiscoveryNetworkBehaviorOpts,
}

impl DiscoveryNetworkBehavior {
    pub fn new(opts: DiscoveryNetworkBehaviorOpts) -> Self {
        let logic_conf = DiscoveryLogicConf {
            local_node_id: opts.local_node_id,
            timer: opts.timer.clone(),
        };
        
        Self {
            logic: Arc::new(Mutex::new(DiscoveryLogic::new(logic_conf))),
            opts
        }
    }

    fn process_logic_actions<BE>(&mut self, agent: &NetworkAgent<BE>) {
        while let Some(action) = self.logic.lock().poll_action() {
            match action {
                Action::ConnectTo(peer_id, addr) => {
                    agent.connect_to(peer_id, addr);
                }
                Action::SendTo(peer_id, msg) => {
                    todo!()
                }
            }
        }
    }
}

impl<BE, HE> NetworkBehavior<BE, HE> for DiscoveryNetworkBehavior
    where BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent>,
          HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent>,
{
    fn on_tick(&mut self, agent: &NetworkAgent<HE>, ts_ms: u64, interal_ms: u64) {
        if let Some(bootstrap) = self.opts.bootstrap_addrs.take() {
            for (peer, addr) in bootstrap {
                self.logic.lock().on_input(Input::AddPeer(peer, addr));
            }
        }
        self.process_logic_actions(agent);
    }

    fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE>>> {
        self.logic.lock().on_input(Input::OnConnected(connection.peer_id(), connection.remote_addr()));
        Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE>>> {
        todo!()
        // self.logic.lock().on_input(Input::OnConnected(connection.peer_id(), connection.remote_addr()));
        // Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) {
        self.logic.lock().on_input(Input::OnDisconnected(connection.peer_id()));
    }

    fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) {
        self.logic.lock().on_input(Input::OnDisconnected(connection.peer_id()));
    }

    fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {
        self.logic.lock().on_input(Input::OnConnectError(peer_id));
    }

    fn on_event(&mut self, agent: &NetworkAgent<HE>, event: NetworkBehaviorEvent) {
        todo!()
    }

    fn on_handler_event(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, event: HE) {
        todo!()
    }
}
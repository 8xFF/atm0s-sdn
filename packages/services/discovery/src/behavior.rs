use crate::handler::DiscoveryConnectionHandler;
use crate::logic::{Action, DiscoveryLogic, DiscoveryLogicConf, Input};
use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg};
use bluesea_identity::{PeerAddr, PeerId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::plane::{BehaviorAgent, CrossHandlerRoute};
use network::transport::{ConnectionMsg, ConnectionSender, OutgoingConnectionError, TransportPendingOutgoing};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use utils::Timer;
use crate::connection_group::ConnectionGrouping;

pub struct DiscoveryNetworkBehaviorOpts {
    pub local_node_id: PeerId,
    pub bootstrap_addrs: Option<Vec<(PeerId, PeerAddr)>>,
    pub timer: Arc<dyn Timer>,
}

pub struct DiscoveryNetworkBehavior {
    logic: DiscoveryLogic,
    opts: DiscoveryNetworkBehaviorOpts,
    connection_group: ConnectionGrouping,
}

impl DiscoveryNetworkBehavior {
    pub fn new(opts: DiscoveryNetworkBehaviorOpts) -> Self {
        let logic_conf = DiscoveryLogicConf {
            local_node_id: opts.local_node_id,
            timer: opts.timer.clone(),
        };

        Self {
            logic: DiscoveryLogic::new(logic_conf),
            connection_group: ConnectionGrouping::default(),
            opts,
        }
    }

    fn process_logic_actions<BE, MSG>(&mut self, agent: &BehaviorAgent<BE, MSG>)
        where
            BE: Send + Sync + 'static,
            MSG: TryInto<DiscoveryMsg> + From<DiscoveryMsg> + Send + Sync + 'static,
    {
        while let Some(action) = self.logic.poll_action() {
            match action {
                Action::ConnectTo(peer_id, addr) => {
                    agent.connect_to(peer_id, addr);
                }
                Action::SendTo(peer_id, msg) => {
                    agent.send_to_net(
                        CrossHandlerRoute::PeerFirst(peer_id),
                        ConnectionMsg::Reliable {
                            stream_id: 0,
                            data: msg.into()
                        }
                    );
                }
            }
        }
    }

    fn add_connection_if_need<HE, MSG>(&mut self,
                              agent: &BehaviorAgent<HE, MSG>,
                              connection: Arc<dyn ConnectionSender<MSG>>)
        where
            HE: Send + Sync + 'static,
            MSG: TryInto<DiscoveryMsg> + From<DiscoveryMsg> + Send + Sync + 'static,
    {
        if self.connection_group.add(connection.remote_peer_id(), connection.connection_id()) {
            self.logic.on_input(Input::OnConnected(
                connection.remote_peer_id(),
                connection.remote_addr(),
            ));
            self.process_logic_actions::<HE, MSG>(agent);
        }
    }

    fn remove_connection_if_need<HE, MSG>(&mut self,
                              agent: &BehaviorAgent<HE, MSG>,
                              connection: Arc<dyn ConnectionSender<MSG>>)
        where
            HE: Send + Sync + 'static,
            MSG: TryInto<DiscoveryMsg> + From<DiscoveryMsg> + Send + Sync + 'static,
    {
        if self.connection_group.remove(connection.remote_peer_id(), connection.connection_id()) {
            self.logic.on_input(Input::OnDisconnected(
                connection.remote_peer_id(),
            ));
            self.process_logic_actions::<HE, MSG>(agent);
        }
    }
}

impl<BE, HE, MSG> NetworkBehavior<BE, HE, MSG> for DiscoveryNetworkBehavior
where
    BE: TryInto<DiscoveryBehaviorEvent<MSG>> + From<DiscoveryBehaviorEvent<MSG>> + Send + Sync + 'static,
    HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent> + Send + Sync + 'static,
    MSG: TryInto<DiscoveryMsg> + From<DiscoveryMsg> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        0
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {
        if let Some(bootstrap) = self.opts.bootstrap_addrs.take() {
            for (peer, addr) in bootstrap {
                self.logic.on_input(Input::AddPeer(peer, addr));
            }
            self.logic.on_input(Input::RefreshKey(self.opts.local_node_id));
        }
        self.logic.on_input(Input::OnTick(ts_ms));
        self.process_logic_actions::<HE, MSG>(agent);
    }

    fn on_incoming_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>> {
        self.add_connection_if_need::<>(agent, connection);
        Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>> {
        self.add_connection_if_need(agent, connection);
        Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) {
        self.remove_connection_if_need(agent, connection);
    }

    fn on_outgoing_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) {
        self.remove_connection_if_need(agent, connection);
    }

    fn on_outgoing_connection_error(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        peer_id: PeerId,
        connection_id: u32,
        err: &OutgoingConnectionError,
    ) {
        self.logic.on_input(Input::OnConnectError(peer_id));
        self.process_logic_actions::<HE, MSG>(agent);
    }

    fn on_handler_event(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        peer_id: PeerId,
        _connection_id: u32,
        event: BE,
    ) {
        match event.try_into() {
            Ok(DiscoveryBehaviorEvent::OnNetworkMessage(msg)) => {
                match msg.try_into() {
                    Ok(msg) => {
                        self.logic.on_input(Input::OnData(peer_id, msg));
                        self.process_logic_actions::<HE, MSG>(agent);
                    },
                    Err(e) => {
                        log::error!("cannot convert to DiscoveryMsg");
                    }
                }
            }
            Err(e) => {
                log::error!("cannot convert to DiscoveryBehaviorEvent");
            }
        }
    }
}

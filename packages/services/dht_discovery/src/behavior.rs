use crate::connection_group::ConnectionGrouping;
use crate::handler::DiscoveryConnectionHandler;
use crate::logic::{Action, DiscoveryLogic, DiscoveryLogicConf, Input};
use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent};
use crate::DISCOVERY_SERVICE_ID;
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use bluesea_router::RouteRule;
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::msg::TransportMsg;
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
use network::BehaviorAgent;
use std::sync::Arc;
use utils::error_handle::ErrorUtils;
use utils::Timer;

pub struct DiscoveryNetworkBehaviorOpts {
    pub local_node_id: NodeId,
    pub bootstrap_addrs: Option<Vec<(NodeId, NodeAddr)>>,
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

    fn process_logic_actions<BE, HE>(&mut self, agent: &BehaviorAgent<BE, HE>)
    where
        BE: Send + Sync + 'static,
        HE: Send + Sync + 'static,
    {
        while let Some(action) = self.logic.poll_action() {
            match action {
                Action::ConnectTo(node_id, addr) => {
                    agent.connect_to(node_id, addr).print_error("Should connect to node");
                }
                Action::SendTo(node_id, msg) => {
                    agent.send_to_net(TransportMsg::build_reliable(DISCOVERY_SERVICE_ID, RouteRule::ToNode(node_id), 0, &bincode::serialize(&msg).unwrap()));
                }
            }
        }
    }

    fn add_connection_if_need<BE, HE>(&mut self, agent: &BehaviorAgent<BE, HE>, connection: Arc<dyn ConnectionSender>)
    where
        BE: Send + Sync + 'static,
        HE: Send + Sync + 'static,
    {
        if self.connection_group.add(connection.remote_node_id(), connection.conn_id()) {
            self.logic.on_input(Input::OnConnected(connection.remote_node_id(), connection.remote_addr()));
            self.process_logic_actions::<BE, HE>(agent);
        }
    }

    fn remove_connection_if_need<BE, HE>(&mut self, agent: &BehaviorAgent<BE, HE>, connection: Arc<dyn ConnectionSender>)
    where
        BE: Send + Sync + 'static,
        HE: Send + Sync + 'static,
    {
        if self.connection_group.remove(connection.remote_node_id(), connection.conn_id()) {
            self.logic.on_input(Input::OnDisconnected(connection.remote_node_id()));
            self.process_logic_actions::<BE, HE>(agent);
        }
    }
}

impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for DiscoveryNetworkBehavior
where
    BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent> + Send + Sync + 'static,
    HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        DISCOVERY_SERVICE_ID
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<BE, HE>, ts_ms: u64, _interal_ms: u64) {
        if let Some(bootstrap) = self.opts.bootstrap_addrs.take() {
            for (node, addr) in bootstrap {
                self.logic.on_input(Input::AddNode(node, addr));
            }
            self.logic.on_input(Input::RefreshKey(self.opts.local_node_id));
        }
        self.logic.on_input(Input::OnTick(ts_ms));
        self.process_logic_actions::<BE, HE>(agent);
    }

    fn check_incoming_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_local_msg(&mut self, _agent: &BehaviorAgent<BE, HE>, _msg: TransportMsg) {
        panic!("Should not happend");
    }

    fn on_incoming_connection_connected(&mut self, agent: &BehaviorAgent<BE, HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.add_connection_if_need(agent, connection);
        Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(&mut self, agent: &BehaviorAgent<BE, HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.add_connection_if_need(agent, connection);
        Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(&mut self, agent: &BehaviorAgent<BE, HE>, connection: Arc<dyn ConnectionSender>) {
        self.remove_connection_if_need(agent, connection);
    }

    fn on_outgoing_connection_disconnected(&mut self, agent: &BehaviorAgent<BE, HE>, connection: Arc<dyn ConnectionSender>) {
        self.remove_connection_if_need(agent, connection);
    }

    fn on_outgoing_connection_error(&mut self, agent: &BehaviorAgent<BE, HE>, node_id: NodeId, _connection_id: ConnId, _err: &OutgoingConnectionError) {
        self.logic.on_input(Input::OnConnectError(node_id));
        self.process_logic_actions::<BE, HE>(agent);
    }

    fn on_handler_event(&mut self, agent: &BehaviorAgent<BE, HE>, node_id: NodeId, _connection_id: ConnId, event: BE) {
        match event.try_into() {
            Ok(DiscoveryBehaviorEvent::OnNetworkMessage(msg)) => {
                self.logic.on_input(Input::OnData(node_id, msg));
                self.process_logic_actions::<BE, HE>(agent);
            }
            Err(_) => {
                log::error!("cannot convert to DiscoveryBehaviorEvent");
            }
        }
    }

    fn on_rpc(&mut self, _agent: &BehaviorAgent<BE, HE>, _req: Req, _res: Box<dyn RpcAnswer<Res>>) -> bool {
        false
    }

    fn on_started(&mut self, _agent: &BehaviorAgent<BE, HE>) {}

    fn on_stopped(&mut self, _agent: &BehaviorAgent<BE, HE>) {}
}

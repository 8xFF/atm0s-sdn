use crate::connection_group::ConnectionGrouping;
use crate::handler::DiscoveryConnectionHandler;
use crate::logic::{Action, DiscoveryLogic, DiscoveryLogicConf, Input};
use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent};
use crate::DISCOVERY_SERVICE_ID;
use p_8xff_sdn_identity::{ConnId, NodeAddr, NodeId};
use p_8xff_sdn_network::behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction};
use p_8xff_sdn_network::msg::TransportMsg;
use p_8xff_sdn_network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid};
use p_8xff_sdn_router::RouteRule;
use p_8xff_sdn_utils::Timer;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct DiscoveryNetworkBehaviorOpts {
    pub local_node_id: NodeId,
    pub bootstrap_addrs: Option<Vec<(NodeId, NodeAddr)>>,
    pub timer: Arc<dyn Timer>,
}

pub struct DiscoveryNetworkBehavior<HE, SE> {
    logic: DiscoveryLogic,
    opts: DiscoveryNetworkBehaviorOpts,
    connection_group: ConnectionGrouping,
    outputs: VecDeque<NetworkBehaviorAction<HE, SE>>,
}

impl<HE, SE> DiscoveryNetworkBehavior<HE, SE>
where
    HE: Send + Sync + 'static,
    SE: Send + Sync + 'static, // TODO: Update SDK Event
{
    pub fn new(opts: DiscoveryNetworkBehaviorOpts) -> Self {
        let logic_conf = DiscoveryLogicConf {
            local_node_id: opts.local_node_id,
            timer: opts.timer.clone(),
        };

        Self {
            logic: DiscoveryLogic::new(logic_conf),
            connection_group: ConnectionGrouping::default(),
            opts,
            outputs: VecDeque::new(),
        }
    }

    fn process_logic_actions<BE>(&mut self, _ctx: &BehaviorContext)
    where
        BE: Send + Sync + 'static,
    {
        while let Some(action) = self.logic.poll_action() {
            match action {
                Action::ConnectTo(node_id, addr) => {
                    self.outputs.push_back(NetworkBehaviorAction::ConnectTo(0, node_id, addr));
                }
                Action::SendTo(node_id, msg) => {
                    let msg = TransportMsg::build_reliable(DISCOVERY_SERVICE_ID, RouteRule::ToNode(node_id), 0, &bincode::serialize(&msg).expect("Should serialize"));
                    self.outputs.push_back(NetworkBehaviorAction::ToNet(msg));
                }
            }
        }
    }

    fn add_connection_if_need<BE>(&mut self, ctx: &BehaviorContext, connection: Arc<dyn ConnectionSender>)
    where
        BE: Send + Sync + 'static,
    {
        if self.connection_group.add(connection.remote_node_id(), connection.conn_id()) {
            self.logic.on_input(Input::OnConnected(connection.remote_node_id(), connection.remote_addr()));
            self.process_logic_actions::<BE>(ctx);
        }
    }

    fn remove_connection_if_need<BE>(&mut self, ctx: &BehaviorContext, node_id: NodeId, conn_id: ConnId)
    where
        BE: Send + Sync + 'static,
    {
        if self.connection_group.remove(node_id, conn_id) {
            self.logic.on_input(Input::OnDisconnected(node_id));
            self.process_logic_actions::<BE>(ctx);
        }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for DiscoveryNetworkBehavior<HE, SE>
where
    BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent> + Send + Sync + 'static,
    HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent> + Send + Sync + 'static,
    SE: Send + Sync + 'static, // TODO: Update SDK Event
{
    fn service_id(&self) -> u8 {
        DISCOVERY_SERVICE_ID
    }

    fn on_tick(&mut self, ctx: &BehaviorContext, ts_ms: u64, _interal_ms: u64) {
        if let Some(bootstrap) = self.opts.bootstrap_addrs.take() {
            for (node, addr) in bootstrap {
                self.logic.on_input(Input::AddNode(node, addr));
            }
            self.logic.on_input(Input::RefreshKey(self.opts.local_node_id));
        }
        self.logic.on_input(Input::OnTick(ts_ms));
        self.process_logic_actions::<BE>(ctx);
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId, _local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _msg: TransportMsg) {
        panic!("Should not happend");
    }

    fn on_incoming_connection_connected(&mut self, ctx: &BehaviorContext, _now_ms: u64, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.add_connection_if_need::<BE>(ctx, connection);
        Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        ctx: &BehaviorContext,
        _now_ms: u64,
        connection: Arc<dyn ConnectionSender>,
        _local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        self.add_connection_if_need::<BE>(ctx, connection);
        Some(Box::new(DiscoveryConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(&mut self, ctx: &BehaviorContext, _now_ms: u64, node_id: NodeId, conn_id: ConnId) {
        self.remove_connection_if_need::<BE>(ctx, node_id, conn_id);
    }

    fn on_outgoing_connection_disconnected(&mut self, ctx: &BehaviorContext, _now_ms: u64, node_id: NodeId, conn_id: ConnId) {
        self.remove_connection_if_need::<BE>(ctx, node_id, conn_id);
    }

    fn on_outgoing_connection_error(
        &mut self,
        ctx: &BehaviorContext,
        _now_ms: u64,
        node_id: NodeId,
        _connection_id: Option<ConnId>,
        _local_uuid: TransportOutgoingLocalUuid,
        _err: &OutgoingConnectionError,
    ) {
        self.logic.on_input(Input::OnConnectError(node_id));
        self.process_logic_actions::<BE>(ctx);
    }

    fn on_handler_event(&mut self, ctx: &BehaviorContext, _now_ms: u64, node_id: NodeId, _connection_id: ConnId, event: BE) {
        match event.try_into() {
            Ok(DiscoveryBehaviorEvent::OnNetworkMessage(msg)) => {
                self.logic.on_input(Input::OnData(node_id, msg));
                self.process_logic_actions::<BE>(ctx);
            }
            Err(_) => {
                log::error!("cannot convert to DiscoveryBehaviorEvent");
            }
        }
    }

    fn on_started(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        None
    }

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}
}

use crate::msg::TransportMsg;
use crate::plane::bus::HandlerRoute;
use crate::transport::{ConnectionEvent, ConnectionRejectReason, ConnectionSender, OutgoingConnectionError};
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_utils::awaker::Awaker;
use std::sync::Arc;

#[cfg(test)]
use mockall::automock;

#[derive(Clone)]
/// A struct representing the context of a behavior.
pub struct BehaviorContext {
    /// The service ID of the behavior.
    pub service_id: u8,
    /// The node ID of the behavior.
    pub node_id: NodeId,
    /// The awaker of the behavior.
    pub awaker: Arc<dyn Awaker>,
}

impl BehaviorContext {
    pub(crate) fn new(service_id: u8, node_id: NodeId, awaker: Arc<dyn Awaker>) -> Self {
        Self { service_id, node_id, awaker }
    }
}

#[derive(Clone)]
pub struct ConnectionContext {
    pub service_id: u8,
    pub local_node_id: NodeId,
    pub remote_node_id: NodeId,
    pub conn_id: ConnId,
    pub awaker: Arc<dyn Awaker>,
}

impl ConnectionContext {
    #[allow(unused)]
    pub(crate) fn new(service_id: u8, local_node_id: NodeId, remote_node_id: NodeId, conn_id: ConnId, awaker: Arc<dyn Awaker>) -> Self {
        Self {
            service_id,
            local_node_id,
            remote_node_id,
            conn_id,
            awaker,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConnectionHandlerAction<BE, HE> {
    /// Send an event to the behavior.
    ToBehaviour(BE),
    /// Send an event to the network.
    ToNet(TransportMsg),
    /// Send an event to the network, specify the destination connection.
    ToNetConn(ConnId, TransportMsg),
    /// Send an event to the network, specify the destination node.
    ToNetNode(NodeId, TransportMsg),
    /// Send an event to the handler.
    ToHandler(HandlerRoute, HE),
    /// Close the connection.
    CloseConn(),
}

/// A trait representing a connection handler.
pub trait ConnectionHandler<BE, HE>: Send + Sync {
    /// Called when the connection is opened.
    fn on_opened(&mut self, ctx: &ConnectionContext, now_ms: u64);

    /// Called on each tick of the connection.
    fn on_tick(&mut self, ctx: &ConnectionContext, now_ms: u64, interval_ms: u64);

    /// Called when the connection is awake.
    fn on_awake(&mut self, ctx: &ConnectionContext, now_ms: u64);

    /// Called when an event occurs on the connection.
    fn on_event(&mut self, ctx: &ConnectionContext, now_ms: u64, event: ConnectionEvent);

    /// Called when an event occurs on another handler.
    fn on_other_handler_event(&mut self, ctx: &ConnectionContext, now_ms: u64, from_node: NodeId, from_conn: ConnId, event: HE);

    /// Called when an event occurs on the behavior.
    fn on_behavior_event(&mut self, ctx: &ConnectionContext, now_ms: u64, event: HE);

    /// Called when the connection is closed.
    fn on_closed(&mut self, ctx: &ConnectionContext, now_ms: u64);

    /// Pops the next action to be taken by the connection handler.
    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>>;
}

#[derive(Debug, PartialEq, Eq)]
/// Enum representing the actions that can be taken by the network behavior.
pub enum NetworkBehaviorAction<HE, SE> {
    /// Enum representing the different types of connections that can be made.
    /// `ConnectTo` variant is used to connect to a node with a given `NodeId` and `NodeAddr`.
    ConnectTo(NodeAddr),
    /// Represents a message sent to the network layer for transport.
    ToNet(TransportMsg),
    /// Represents a message sent to the network layer for transport, specifying the destination connection.
    ToNetConn(ConnId, TransportMsg),
    /// Represents a message sent to the network layer for transport, specifying the destination node.
    ToNetNode(NodeId, TransportMsg),
    /// Represents a message sent to the handler.
    ToHandler(HandlerRoute, HE),
    /// Represents a message sent to a sdk service.
    ToSdkService(u8, SE),
    /// Represents a Close connection action.
    CloseConn(ConnId),
    /// Represents a Close node action.
    CloseNode(NodeId),
}

#[cfg_attr(test, automock)]
/// Defines the behavior of a network service.
pub trait NetworkBehavior<BE, HE, SE> {
    /// Returns the service ID of the behavior.
    fn service_id(&self) -> u8;

    /// Called when the behavior is started.
    fn on_started(&mut self, ctx: &BehaviorContext, now_ms: u64);

    /// Called on each tick of the behavior.
    fn on_tick(&mut self, ctx: &BehaviorContext, now_ms: u64, interval_ms: u64);

    /// Called when the behavior is awoken.
    fn on_awake(&mut self, ctx: &BehaviorContext, now_ms: u64);

    /// Called when a message is received from other SDK.
    fn on_sdk_msg(&mut self, ctx: &BehaviorContext, now_ms: u64, from_service: u8, event: SE);

    /// Called when a message is received locally.
    fn on_local_msg(&mut self, ctx: &BehaviorContext, now_ms: u64, msg: TransportMsg);

    /// Called when an incoming connection is received.
    fn check_incoming_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason>;

    /// Called when an outgoing connection is initiated.
    fn check_outgoing_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason>;

    /// Called when an incoming connection is established.
    fn on_incoming_connection_connected(&mut self, ctx: &BehaviorContext, now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>>;

    /// Called when an outgoing connection is established.
    fn on_outgoing_connection_connected(
        &mut self,
        ctx: &BehaviorContext,
        now_ms: u64,
        conn: Arc<dyn ConnectionSender>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>>;

    /// Called when an incoming connection is disconnected.
    fn on_incoming_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId);

    /// Called when an outgoing connection is disconnected.
    fn on_outgoing_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId);

    /// Called when an outgoing connection encounters an error.
    fn on_outgoing_connection_error(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError);

    /// Called when a handler event is received.
    fn on_handler_event(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId, event: BE);

    /// Called when the behavior is stopped.
    fn on_stopped(&mut self, ctx: &BehaviorContext, now_ms: u64);

    /// Pops the next action from the behavior's action queue.
    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>>;
}

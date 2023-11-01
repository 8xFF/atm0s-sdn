use crate::msg::TransportMsg;
use crate::plane::bus::HandlerRoute;
use crate::transport::{ConnectionEvent, ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, TransportOutgoingLocalUuid};
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use std::sync::Arc;
use utils::awaker::Awaker;

#[derive(Clone)]
pub struct BehaviorContext {
    pub service_id: u8,
    pub node_id: NodeId,
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
    ToBehaviour(BE),
    ToNet(TransportMsg),
    ToNetConn(ConnId, TransportMsg),
    ToNetNode(NodeId, TransportMsg),
    ToHandler(HandlerRoute, HE),
    CloseConn(),
}

pub trait ConnectionHandler<BE, HE>: Send + Sync {
    fn on_opened(&mut self, ctx: &ConnectionContext, now_ms: u64);
    fn on_tick(&mut self, ctx: &ConnectionContext, now_ms: u64, interval_ms: u64);
    fn on_awake(&mut self, ctx: &ConnectionContext, now_ms: u64);
    fn on_event(&mut self, ctx: &ConnectionContext, now_ms: u64, event: ConnectionEvent);
    fn on_other_handler_event(&mut self, ctx: &ConnectionContext, now_ms: u64, from_node: NodeId, from_conn: ConnId, event: HE);
    fn on_behavior_event(&mut self, ctx: &ConnectionContext, now_ms: u64, event: HE);
    fn on_closed(&mut self, ctx: &ConnectionContext, now_ms: u64);
    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum NetworkBehaviorAction<HE> {
    ConnectTo(TransportOutgoingLocalUuid, NodeId, NodeAddr),
    ToNet(TransportMsg),
    ToNetConn(ConnId, TransportMsg),
    ToNetNode(NodeId, TransportMsg),
    ToHandler(HandlerRoute, HE),
    CloseConn(ConnId),
    CloseNode(NodeId),
}

pub trait NetworkBehavior<BE, HE> {
    fn service_id(&self) -> u8;
    fn on_started(&mut self, ctx: &BehaviorContext, now_ms: u64);
    fn on_tick(&mut self, ctx: &BehaviorContext, now_ms: u64, interval_ms: u64);
    fn on_awake(&mut self, ctx: &BehaviorContext, now_ms: u64);
    fn on_local_msg(&mut self, ctx: &BehaviorContext, now_ms: u64, msg: TransportMsg);
    fn on_local_event(&mut self, ctx: &BehaviorContext, now_ms: u64, event: BE);
    fn check_incoming_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason>;
    fn check_outgoing_connection(&mut self, ctx: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId, local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason>;
    fn on_incoming_connection_connected(&mut self, ctx: &BehaviorContext, now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>>;
    fn on_outgoing_connection_connected(
        &mut self,
        ctx: &BehaviorContext,
        now_ms: u64,
        conn: Arc<dyn ConnectionSender>,
        local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>>;
    fn on_incoming_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId);
    fn on_outgoing_connection_disconnected(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId);
    fn on_outgoing_connection_error(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: Option<ConnId>, local_uuid: TransportOutgoingLocalUuid, err: &OutgoingConnectionError);
    fn on_handler_event(&mut self, ctx: &BehaviorContext, now_ms: u64, node_id: NodeId, conn_id: ConnId, event: BE);
    fn on_stopped(&mut self, ctx: &BehaviorContext, now_ms: u64);
    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE>>;
}

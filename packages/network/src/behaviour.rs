use crate::internal::agent::{BehaviorAgent, ConnectionAgent};
use crate::transport::{ConnectionEvent, ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
use bluesea_identity::{ConnId, NodeId};
use std::sync::Arc;

pub trait ConnectionHandler<BE, HE>: Send + Sync {
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE>);
    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE>, ts_ms: u64, interval_ms: u64);
    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent);
    fn on_other_handler_event(&mut self, agent: &ConnectionAgent<BE, HE>, from_node: NodeId, from_conn: ConnId, event: HE);
    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: HE);
    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE>);
}

pub trait NetworkBehavior<BE, HE, Req, Res> {
    fn service_id(&self) -> u8;
    fn on_tick(&mut self, agent: &BehaviorAgent<HE>, ts_ms: u64, interal_ms: u64);
    fn check_incoming_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason>;
    fn check_outgoing_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason>;
    fn on_incoming_connection_connected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>>;
    fn on_outgoing_connection_connected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>>;
    fn on_incoming_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>);
    fn on_outgoing_connection_error(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError);
    fn on_handler_event(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, event: BE);
    fn on_rpc(&mut self, agent: &BehaviorAgent<HE>, req: Req, res: Box<dyn RpcAnswer<Res>>) -> bool;
}

use crate::msg::TransportMsg;
use bluesea_identity::{ConnId, NodeId};

pub(crate) mod agent;
pub(crate) mod cross_handler_gate;

pub(crate) enum CrossHandlerEvent<HE> {
    FromBehavior(HE),
    FromHandler(NodeId, ConnId, HE),
}

#[derive(Debug, PartialEq, Eq)]
pub enum CrossHandlerRoute {
    NodeFirst(NodeId),
    Conn(ConnId),
}

pub(crate) trait CrossHandlerGate<BE, HE>: Send + Sync {
    fn close_conn(&self, conn: ConnId);
    fn close_node(&self, node: NodeId);
    fn send_to_behaviour(&self, service_id: u8, event: BE) -> Option<()>;
    fn send_to_handler(&self, service_id: u8, route: CrossHandlerRoute, event: CrossHandlerEvent<HE>) -> Option<()>;
    fn send_to_net(&self, msg: TransportMsg) -> Option<()>;
    fn send_to_net_node(&self, node: NodeId, msg: TransportMsg) -> Option<()>;
    fn send_to_net_direct(&self, conn_id: ConnId, msg: TransportMsg) -> Option<()>;
}

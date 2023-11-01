use bluesea_identity::{ConnId, NodeId};

use crate::msg::TransportMsg;

pub(crate) enum HandleEvent<HE> {
    Awake,
    FromBehavior(HE),
    FromHandler(NodeId, ConnId, HE),
}

#[derive(Debug, PartialEq, Eq)]
pub enum HandlerRoute {
    NodeFirst(NodeId),
    Conn(ConnId),
}

pub(crate) trait PlaneBus<BE, HE>: Send + Sync {
    fn awake_behaviour(&self, service: u8) -> Option<()>;
    fn awake_handler(&self, service: u8, conn: ConnId) -> Option<()>;
    fn to_behaviour(&self, service_id: u8, event: BE) -> Option<()>;
    fn to_handler(&self, service_id: u8, route: HandlerRoute, event: HandleEvent<HE>) -> Option<()>;
    fn to_net(&self, msg: TransportMsg) -> Option<()>;
    fn to_net_node(&self, node: NodeId, msg: TransportMsg) -> Option<()>;
    fn to_net_conn(&self, conn_id: ConnId, msg: TransportMsg) -> Option<()>;
}

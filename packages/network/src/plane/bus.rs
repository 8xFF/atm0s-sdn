use atm0s_sdn_identity::{ConnId, NodeId};

use crate::msg::TransportMsg;

#[derive(Debug, PartialEq, Eq)]
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

/// A trait representing a plane bus that is both Send and Sync.
/// This is used to send messages between the different layers (network, behavior and handler).
pub(crate) trait PlaneBus<BE, HE>: Send + Sync {
    /// Trigger the internal awake behaviour event for the given service.
    /// This will call the on_awaker() method of the behaviour.
    fn awake_behaviour(&self, service: u8) -> Option<()>;
    /// Sends an Awake Handler event to the given service of a connection.
    fn awake_handler(&self, service: u8, conn: ConnId) -> Option<()>;
    /// Forward the given event from the handler to the behaviour layer.
    fn to_behaviour_from_handler(&self, service_id: u8, node_id: NodeId, conn_id: ConnId, event: BE) -> Option<()>;
    /// Sends an Event to the Handler of the given service by connection Id or node Id.
    fn to_handler(&self, service_id: u8, route: HandlerRoute, event: HandleEvent<HE>) -> Option<()>;
    /// Sends a Message to the network layer.
    fn to_net(&self, msg: TransportMsg) -> Option<()>;
    /// Sends a Message to the network layer, specify the destination node
    fn to_net_node(&self, node: NodeId, msg: TransportMsg) -> Option<()>;
    /// Sends a Message to the network layer, specify the destination connection
    fn to_net_conn(&self, conn_id: ConnId, msg: TransportMsg) -> Option<()>;
}

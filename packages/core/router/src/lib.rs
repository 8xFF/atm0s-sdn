mod force_local;
mod force_node;

use bluesea_identity::{ConnId, NodeId};
pub use force_local::ForceLocalRouter;
pub use force_node::ForceNodeRouter;

#[cfg(any(test, feature = "mock"))]
use mockall::automock;

/// ServiceMeta is using for determine which node will be routed, example node with lowest price or lowest latency, which for future use
pub type ServiceMeta = u32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteRule {
    Direct,
    ToNode(NodeId),
    ToService(ServiceMeta),
    ToKey(NodeId),
}

/// Determine the destination of an action/message
pub enum RouteAction {
    /// Reject the message
    Reject,
    /// Will be processed locally
    Local,
    /// Will be forward to the given node
    Next(ConnId, NodeId),
}

impl RouteAction {
    pub fn is_local(&self) -> bool {
        matches!(self, RouteAction::Local)
    }

    pub fn is_reject(&self) -> bool {
        matches!(self, RouteAction::Reject)
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, RouteAction::Next(_, _))
    }
}

#[cfg_attr(any(test, feature = "mock"), automock)]
pub trait RouterTable: Send + Sync {
    /// Determine the next action for the given destination node
    fn path_to_node(&self, dest: NodeId) -> RouteAction;
    /// Determine the next action for the given key
    fn path_to_key(&self, key: NodeId) -> RouteAction;
    /// Determine the next action for the given service
    fn path_to_service(&self, service_id: u8) -> RouteAction;
    /// Determine next action for incoming messages
    /// given the route rule and service id
    fn derive_action(&self, route: &RouteRule, service_id: u8) -> RouteAction {
        match route {
            RouteRule::Direct => RouteAction::Local,
            RouteRule::ToNode(dest) => self.path_to_node(*dest),
            RouteRule::ToKey(key) => self.path_to_key(*key),
            RouteRule::ToService(_) => self.path_to_service(service_id),
        }
    }
}

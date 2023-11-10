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
#[derive(Clone, Debug, Eq, PartialEq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_local() {
        let local = RouteAction::Local;
        let remote = RouteAction::Next(ConnId::from_in(1, 1), 2);
        let reject = RouteAction::Reject;

        assert!(local.is_local());
        assert!(!remote.is_local());
        assert!(!reject.is_local());
    }

    #[test]
    fn test_is_reject() {
        let local = RouteAction::Local;
        let remote = RouteAction::Next(ConnId::from_in(1, 1), 2);
        let reject = RouteAction::Reject;

        assert!(!local.is_reject());
        assert!(!remote.is_reject());
        assert!(reject.is_reject());
    }

    #[test]
    fn test_is_remote() {
        let local = RouteAction::Local;
        let remote = RouteAction::Next(ConnId::from_in(1, 1), 2);
        let reject = RouteAction::Reject;

        assert!(!local.is_remote());
        assert!(remote.is_remote());
        assert!(!reject.is_remote());
    }

    #[test]
    fn test_derive_action_to_service() {
        let router = ForceLocalRouter();
        let route = RouteRule::ToService(3);
        let service_id = 1;

        assert_eq!(router.derive_action(&route, service_id), RouteAction::Local);
    }
}

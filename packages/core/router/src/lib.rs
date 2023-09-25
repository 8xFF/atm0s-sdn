mod force_local;
mod force_node;

use bluesea_identity::{ConnId, NodeId};
pub use force_local::ForceLocalRouter;
pub use force_node::ForceNodeRouter;

/// ServiceMeta is using for determine which node will be routed, example node with lowest price or lowest latency, which for future use
pub type ServiceMeta = u32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteRule {
    Direct,
    ToNode(NodeId),
    ToService(ServiceMeta),
    ToKey(NodeId),
}

pub enum RouteAction {
    Reject,
    Local,
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

pub trait RouterTable: Send + Sync {
    fn path_to_node(&self, dest: NodeId) -> RouteAction;
    fn path_to_key(&self, key: NodeId) -> RouteAction;
    fn path_to_service(&self, service_id: u8) -> RouteAction;
    fn path_to(&self, route: &RouteRule, service_id: u8) -> RouteAction {
        match route {
            RouteRule::Direct => RouteAction::Local,
            RouteRule::ToNode(dest) => self.path_to_node(*dest),
            RouteRule::ToKey(key) => self.path_to_key(*key),
            RouteRule::ToService(_) => self.path_to_service(service_id),
        }
    }
}

use bluesea_identity::{ConnId, NodeId};

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
        match self {
            RouteAction::Local => true,
            _ => false,
        }
    }

    pub fn is_reject(&self) -> bool {
        match self {
            RouteAction::Reject => true,
            _ => false,
        }
    }

    pub fn is_remote(&self) -> bool {
        match self {
            RouteAction::Next(_, _) => true,
            _ => false,
        }
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

pub struct ForceLocalRouter();

impl RouterTable for ForceLocalRouter {
    fn path_to_node(&self, _dest: NodeId) -> RouteAction {
        RouteAction::Local
    }

    fn path_to_key(&self, _key: NodeId) -> RouteAction {
        RouteAction::Local
    }

    fn path_to_service(&self, _service_id: u8) -> RouteAction {
        RouteAction::Local
    }
}

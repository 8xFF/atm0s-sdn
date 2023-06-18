use bluesea_identity::{ConnId, NodeId};
use crate::msg::MsgRoute;

pub enum RouteAction {
    Reject,
    Local,
    Remote(ConnId, NodeId),
}

pub trait RouterTable: Send + Sync {
    fn path_to_node(&self, dest: NodeId) -> RouteAction;
    fn path_to_key(&self, key: NodeId) -> RouteAction;
    fn path_to_service(&self, service_id: u8) -> RouteAction;
    fn path_to(&self, route: &MsgRoute, service_id: u8) -> RouteAction {
        match route {
            MsgRoute::ToNode(dest) => self.path_to_node(*dest),
            MsgRoute::ToKey(key) => self.path_to_key(*key),
            MsgRoute::ToService(_) => self.path_to_service(service_id),
        }
    }
}

pub struct ForceLocalRouter();

impl RouterTable for ForceLocalRouter {
    fn path_to_node(&self, dest: NodeId) -> RouteAction {
        RouteAction::Local
    }

    fn path_to_key(&self, key: NodeId) -> RouteAction {
        RouteAction::Local
    }

    fn path_to_service(&self, service_id: u8) -> RouteAction {
        RouteAction::Local
    }
}

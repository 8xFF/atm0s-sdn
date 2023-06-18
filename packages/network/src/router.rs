use crate::transport::MsgRoute;
use bluesea_identity::{ConnId, NodeId};

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
            MsgRoute::Node(dest) => self.path_to_node(*dest),
            MsgRoute::Closest(key) => self.path_to_key(*key),
            MsgRoute::Service => self.path_to_service(service_id),
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

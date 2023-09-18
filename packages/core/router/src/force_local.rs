use bluesea_identity::NodeId;

use crate::{RouteAction, RouterTable};

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

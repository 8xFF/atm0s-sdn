use bluesea_identity::{ConnId, NodeId};

use crate::{RouteAction, RouterTable};

pub struct ForceNodeRouter(pub ConnId, pub NodeId);

impl RouterTable for ForceNodeRouter {
    fn path_to_node(&self, _dest: NodeId) -> RouteAction {
        RouteAction::Next(self.0, self.1)
    }

    fn path_to_key(&self, _key: NodeId) -> RouteAction {
        RouteAction::Next(self.0, self.1)
    }

    fn path_to_service(&self, _service_id: u8) -> RouteAction {
        RouteAction::Next(self.0, self.1)
    }
}

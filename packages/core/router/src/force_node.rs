///! ForceNodeRouter is a router that always routes to a specific node.
///! This is useful for testing.
use p_8xff_sdn_identity::{ConnId, NodeId};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_node() {
        let router = ForceNodeRouter(ConnId::from_in(1, 1), NodeId::from(2u32));
        let dest = NodeId::from(3u32);
        assert_eq!(router.path_to_node(dest), RouteAction::Next(ConnId::from_in(1, 1), NodeId::from(2u32)));
    }

    #[test]
    fn test_path_to_key() {
        let router = ForceNodeRouter(ConnId::from_in(1, 1), NodeId::from(2u32));
        let key = NodeId::from(4u32);
        assert_eq!(router.path_to_key(key), RouteAction::Next(ConnId::from_in(1, 1), NodeId::from(2u32)));
    }

    #[test]
    fn test_path_to_service() {
        let router = ForceNodeRouter(ConnId::from_in(1, 1), NodeId::from(2u32));
        let service_id = 5;
        assert_eq!(router.path_to_service(service_id), RouteAction::Next(ConnId::from_in(1, 1), NodeId::from(2u32)));
    }
}

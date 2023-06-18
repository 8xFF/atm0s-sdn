use crate::router::{Router, RouterSync};
use crate::table::{Metric, Path};
use crate::{ServiceDestination};
use bluesea_identity::{ConnId, NodeId};
use parking_lot::RwLock;
use std::sync::Arc;

#[derive(Clone)]
pub struct SharedRouter {
    router: Arc<RwLock<Router>>,
}

impl SharedRouter {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            router: Arc::new(RwLock::new(Router::new(node_id))),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.router.read().node_id()
    }

    pub fn size(&self) -> usize {
        self.router.read().size()
    }

    pub fn register_service(&self, service_id: u8) {
        self.router.write().register_service(service_id)
    }

    pub fn service_next(
        &self,
        service_id: u8,
        excepts: &Vec<NodeId>,
    ) -> Option<ServiceDestination> {
        self.router.read().service_next(service_id, excepts)
    }

    pub fn set_direct(&self, over: ConnId, over_node: NodeId, metric: Metric) {
        self.router.write().set_direct(over, over_node, metric);
    }

    pub fn del_direct(&self, over: ConnId) {
        self.router.write().del_direct(over);
    }

    pub fn next(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<(ConnId, NodeId)> {
        self.router.read().next(dest, excepts)
    }

    pub fn next_path(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<Path> {
        self.router.read().next_path(dest, excepts)
    }

    pub fn closest_node(&self, key: NodeId, excepts: &Vec<NodeId>) -> Option<(ConnId, NodeId, u8, u8)> {
        self.router.read().closest_node(key, excepts)
    }

    pub fn create_sync(&self, for_node: NodeId) -> RouterSync {
        self.router.read().create_sync(for_node)
    }

    pub fn apply_sync(&self, conn: ConnId, src: NodeId, src_send_metric: Metric, sync: RouterSync) {
        self.router
            .write()
            .apply_sync(conn, src, src_send_metric, sync);
    }

    pub fn dump(&self) {
        self.router.read().dump();
    }
}

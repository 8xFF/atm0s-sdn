use crate::{earth::VnetEarth, VNET_PROTOCOL_ID};
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId, Protocol};
use atm0s_sdn_network::transport::TransportConnector;
use std::{
    collections::HashMap,
    sync::{atomic::AtomicU64, Arc},
};

pub struct VnetConnector {
    pub(crate) conn_uuid_seed: AtomicU64,
    pub(crate) local_node: u32,
    pub(crate) earth: Arc<VnetEarth>,
    pub(crate) waiting: HashMap<ConnId, NodeId>,
}

impl VnetConnector {
    pub fn new(local_node: u32, eart: Arc<VnetEarth>) -> Self {
        Self {
            conn_uuid_seed: AtomicU64::new(0),
            local_node,
            earth: eart,
            waiting: HashMap::new(),
        }
    }
}

impl TransportConnector for VnetConnector {
    fn create_pending_outgoing(&mut self, dest: NodeAddr) -> Vec<ConnId> {
        let uuid = self.conn_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let conn_id = ConnId::from_out(VNET_PROTOCOL_ID, uuid);
        self.waiting.insert(conn_id, dest.node_id());
        vec![conn_id]
    }

    fn continue_pending_outgoing(&mut self, conn_id: ConnId) {
        if let Some(node) = self.waiting.remove(&conn_id) {
            self.earth.create_outgoing(self.local_node, node);
        }
    }

    fn destroy_pending_outgoing(&mut self, conn_id: ConnId) {
        self.waiting.remove(&conn_id);
    }
}

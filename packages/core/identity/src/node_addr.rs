use crate::node_id::NodeId;
use parking_lot::Mutex;
pub type NodeAddr = p_8xff_sdn_multiaddr::Multiaddr;
pub use p_8xff_sdn_multiaddr::Protocol;

pub trait NodeAddrType {
    fn node_id(&self) -> Option<NodeId>;
}

impl NodeAddrType for NodeAddr {
    fn node_id(&self) -> Option<NodeId> {
        for protocol in self.iter() {
            if let Protocol::P2p(node_id) = protocol {
                return Some(node_id);
            }
        }
        None
    }
}

/// A builder for creating `NodeAddr` instances.
pub struct NodeAddrBuilder {
    addr: Mutex<NodeAddr>,
}

impl Default for NodeAddrBuilder {
    fn default() -> Self {
        Self { addr: Mutex::new(NodeAddr::empty()) }
    }
}

impl NodeAddrBuilder {
    /// Adds a protocol to the node address.
    pub fn add_protocol(&self, protocol: Protocol) {
        self.addr.lock().push(protocol);
    }

    /// Get the node address.
    pub fn addr(&self) -> NodeAddr {
        (*self.addr.lock()).clone()
    }
}

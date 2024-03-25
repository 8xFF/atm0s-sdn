use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeId};

use crate::base::SecureContext;

pub struct DataPlaneConnection {
    node: NodeId,
    conn: ConnId,
    addr: SocketAddr,
    secure: SecureContext,
}

impl DataPlaneConnection {
    pub fn new(node: NodeId, conn: ConnId, addr: SocketAddr, secure: SecureContext) -> Self {
        Self { node, conn, addr, secure }
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn conn(&self) -> ConnId {
        self.conn
    }
}

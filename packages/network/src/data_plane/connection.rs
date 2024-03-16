use std::net::SocketAddr;

use atm0s_sdn_identity::ConnId;

use crate::base::SecureContext;

pub struct DataPlaneConnection {
    conn: ConnId,
    addr: SocketAddr,
    secure: SecureContext,
}

impl DataPlaneConnection {
    pub fn new(conn: ConnId, addr: SocketAddr, secure: SecureContext) -> Self {
        Self { conn, addr, secure }
    }

    pub fn conn(&self) -> ConnId {
        self.conn
    }
}

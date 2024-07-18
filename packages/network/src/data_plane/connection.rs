use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeId};

use crate::base::{BufferMut, SecureContext, TransportMsgHeader};

pub struct DataPlaneConnection {
    node: NodeId,
    conn: ConnId,
    #[allow(unused)]
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

    /// This will encrypt without first byte, which is used for TransportMsgHeader meta
    pub fn encrypt_if_need(&mut self, now: u64, buf: &mut BufferMut<'_>) -> Option<()> {
        if buf.len() < 1 {
            return None;
        }
        if !TransportMsgHeader::is_secure(buf[0]) {
            return Some(());
        }
        buf.ensure_back(12 + 16); //TODO remove magic numbers
        buf.move_front_right(1);
        self.secure.encryptor.encrypt(now, buf).ok()?;
        buf.move_front_left(1);
        Some(())
    }

    /// This will encrypt without first byte, which is used for TransportMsgHeader meta
    pub fn decrypt_if_need(&mut self, now: u64, buf: &mut BufferMut<'_>) -> Option<()> {
        if buf.len() < 1 {
            return None;
        }
        if !TransportMsgHeader::is_secure(buf[0]) {
            return Some(());
        }
        buf.move_front_right(1);
        self.secure.decryptor.decrypt(now, buf).ok()?;
        buf.move_front_left(1);
        Some(())
    }
}

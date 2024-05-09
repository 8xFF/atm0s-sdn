mod control;
mod feature;
mod msg;
mod secure;
mod service;

use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeId};
pub use control::*;
pub use feature::*;
pub use msg::*;
pub use sans_io_runtime::Buffer;
pub use secure::*;
pub use service::*;

#[derive(Debug, Clone)]
pub struct ConnectionCtx {
    pub conn: ConnId,
    pub node: NodeId,
    pub remote: SocketAddr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionStats {
    pub rtt_ms: u32,
}

#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Connected(ConnectionCtx, SecureContext),
    Stats(ConnectionCtx, ConnectionStats),
    Disconnected(ConnectionCtx),
}

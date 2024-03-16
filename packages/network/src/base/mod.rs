mod buf;
mod control;
mod feature;
mod msg;
mod secure;
mod service;

use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeId};
pub use buf::*;
pub use control::*;
pub use feature::*;
pub use msg::*;
pub use secure::*;
pub use service::*;

#[derive(Debug, Clone)]
pub struct ConnectionCtx {
    pub session: u64,
    pub conn: ConnId,
    pub node: NodeId,
    pub remote: SocketAddr,
}

#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub rtt_ms: u32,
}

#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Connected(ConnectionCtx),
    Stats(ConnectionCtx, ConnectionStats),
    Disconnected(ConnectionCtx),
}

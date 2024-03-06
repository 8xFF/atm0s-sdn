use std::net::SocketAddr;

use super::TransportProtocol;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ConnAddr (pub TransportProtocol, pub SocketAddr);
use std::{net::SocketAddrV4, sync::Arc};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouterTable;

use self::{internal::VirtualNetInternal, udp_socket::VirtualUdpSocket};

mod async_queue;
pub(crate) mod internal;
pub(crate) mod udp_socket;

#[derive(Debug, PartialEq, Clone)]
pub struct VirtualSocketPkt {
    pub src: SocketAddrV4,
    pub payload: Vec<u8>,
    /// ecn, only 2 bits
    pub ecn: Option<u8>,
}

#[derive(Debug, PartialEq)]
pub enum VirtualNetError {
    QueueFull,
    Unreachable,
    AllreadyExists,
    NoAvailablePort,
}

#[derive(Clone)]
pub struct VirtualNet {
    pub(crate) internal: VirtualNetInternal,
}

impl VirtualNet {
    pub(crate) fn new(node_id: NodeId, router: Arc<dyn RouterTable>) -> (Self, VirtualNetInternal) {
        log::info!("[VirtualNet] Create new virtual socket service");
        let internal = VirtualNetInternal::new(node_id, router);
        let net = Self { internal: internal.clone() };
        (net, internal)
    }

    pub fn create_udp_socket(&self, port: u16, buffer_size: usize) -> Result<VirtualUdpSocket, VirtualNetError> {
        Ok(VirtualUdpSocket::new(self.internal.clone(), port, buffer_size)?)
    }
}

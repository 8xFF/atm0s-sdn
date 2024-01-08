use std::{
    net::{SocketAddr, SocketAddrV4},
    sync::Arc,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouterTable;
use behavior::VirtualSocketBehavior;

pub(crate) const VIRTUAL_SOCKET_SERVICE_ID: u8 = 6;

mod behavior;
mod handler;
#[cfg(feature = "quinn")]
mod quinn_utils;
mod vnet;

#[cfg(feature = "quinn")]
pub use quinn;
#[cfg(feature = "quinn")]
pub use quinn_utils::{make_insecure_quinn_client, make_insecure_quinn_server};
pub use vnet::{udp_socket::VirtualUdpSocket, VirtualNet, VirtualNetError, VirtualSocketPkt};

pub fn create_vnet(node_id: NodeId, router: Arc<dyn RouterTable>) -> (VirtualSocketBehavior, vnet::VirtualNet) {
    let (net, interal) = vnet::VirtualNet::new(node_id, router);
    let behavior = VirtualSocketBehavior::new(interal);
    (behavior, net)
}

pub fn vnet_addr_v4(node_id: NodeId, port: u16) -> SocketAddrV4 {
    SocketAddrV4::new(node_id.into(), port)
}

pub fn vnet_addr(node_id: NodeId, port: u16) -> SocketAddr {
    SocketAddr::V4(vnet_addr_v4(node_id, port))
}

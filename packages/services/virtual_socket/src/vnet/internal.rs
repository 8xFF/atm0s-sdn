use std::{collections::HashMap, net::SocketAddrV4, sync::Arc};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    msg::{MsgHeader, TransportMsg},
    transport::ConnectionSender,
};
use atm0s_sdn_router::{RouteAction, RouteRule, RouterTable};
use parking_lot::RwLock;

use crate::{VirtualSocketPkt, VIRTUAL_SOCKET_SERVICE_ID};

use super::{async_queue::AsyncQueue, VirtualNetError};

#[derive(Clone)]
pub struct VirtualNetInternal {
    node_id: NodeId,
    router: Arc<dyn RouterTable>,
    conns: Arc<RwLock<HashMap<ConnId, Arc<dyn ConnectionSender>>>>,
    sockets: Arc<RwLock<HashMap<u16, AsyncQueue<VirtualSocketPkt>>>>,
    ports: Arc<RwLock<Vec<u16>>>,
}

impl VirtualNetInternal {
    pub fn new(node_id: NodeId, router: Arc<dyn RouterTable>) -> Self {
        Self {
            node_id,
            router,
            conns: Default::default(),
            sockets: Default::default(),
            ports: Arc::new(RwLock::new((1..=65535).collect())),
        }
    }

    pub fn local_node(&self) -> NodeId {
        self.node_id
    }

    pub fn register_socket(&self, mut port: u16, buffer_size: usize) -> Result<(AsyncQueue<VirtualSocketPkt>, u16), VirtualNetError> {
        let queue = AsyncQueue::new(buffer_size);
        let mut sockets = self.sockets.write();
        let mut ports = self.ports.write();
        if port == 0 {
            port = *ports.last().ok_or(VirtualNetError::NoAvailablePort)?;
            log::info!("[VirtualNetInternal] No port specified, using {}", port)
        }
        if sockets.contains_key(&port) {
            return Err(VirtualNetError::AllreadyExists);
        }
        log::info!("[VirtualNetInternal] Register socket on port {}", port);
        sockets.insert(port, queue.clone());
        ports.pop();
        Ok((queue, port))
    }

    pub fn unregister_socket(&self, port: u16) {
        let mut sockets = self.sockets.write();
        let mut ports = self.ports.write();
        if sockets.remove(&port).is_some() {
            log::info!("[VirtualNetInternal] Unregister socket on port {}", port);
            ports.push(port);
        }
    }

    pub fn add_conn(&self, conn: Arc<dyn ConnectionSender>) {
        self.conns.write().insert(conn.conn_id(), conn);
    }

    pub fn remove_conn(&self, conn_id: ConnId) {
        self.conns.write().remove(&conn_id);
    }

    pub fn on_incomming(&self, msg: TransportMsg) {
        let from_port = (msg.header.stream_id >> 16) as u16;
        let dest_port = (msg.header.stream_id & 0xFFFF) as u16;
        if let Some(from_node) = msg.header.from_node {
            if let Some(sender) = self.sockets.read().get(&dest_port) {
                if let Err(_e) = sender.try_push(VirtualSocketPkt {
                    src: SocketAddrV4::new(from_node.into(), from_port),
                    payload: msg.payload().to_vec(),
                    ecn: if msg.header.meta == 0b11 {
                        None
                    } else {
                        Some(msg.header.meta)
                    },
                }) {
                    log::warn!("Error sending to queue socket {} full", dest_port);
                }
            } else {
                log::trace!("No socket for port {}", dest_port);
            }
        }
    }

    pub fn send_to(&self, from: u16, dest: SocketAddrV4, payload: &[u8], ecn: Option<u8>) -> Result<(), VirtualNetError> {
        let dest_node: NodeId = (*dest.ip()).into();
        let dest_port = dest.port();
        self.send_to_node(from, dest_node, dest_port, payload, ecn)
    }

    pub fn send_to_node(&self, from: u16, dest_node: NodeId, dest_port: u16, payload: &[u8], ecn: Option<u8>) -> Result<(), VirtualNetError> {
        let rule = RouteRule::ToNode(dest_node);
        match self.router.derive_action(&rule, VIRTUAL_SOCKET_SERVICE_ID) {
            RouteAction::Local => {
                if let Some(sender) = self.sockets.read().get(&dest_port) {
                    sender
                        .try_push(VirtualSocketPkt {
                            src: SocketAddrV4::new(self.node_id.into(), from),
                            payload: payload.to_vec(),
                            ecn,
                        })
                        .map_err(|_| VirtualNetError::QueueFull)?;
                    log::trace!("[VirtualNetInternal] Send {} bytes from {} to {}:{} via local socket", payload.len(), from, dest_node, dest_port);
                    Ok(())
                } else {
                    Err(VirtualNetError::Unreachable)
                }
            }
            RouteAction::Next(conn_id, _) => {
                if let Some(sender) = self.conns.read().get(&conn_id) {
                    let stream_id = (from as u32) << 16 | (dest_port as u32);
                    let header = MsgHeader::build(VIRTUAL_SOCKET_SERVICE_ID, VIRTUAL_SOCKET_SERVICE_ID, rule)
                        .set_from_node(Some(self.node_id))
                        .set_secure(false)
                        .set_meta(ecn.unwrap_or(0b11))
                        .set_stream_id(stream_id);
                    let msg = TransportMsg::build_raw(header, payload);
                    sender.send(msg);
                    Ok(())
                } else {
                    Err(VirtualNetError::Unreachable)
                }
            }
            RouteAction::Reject => Err(VirtualNetError::Unreachable),
        }
    }
}

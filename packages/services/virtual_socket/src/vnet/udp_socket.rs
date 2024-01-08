use std::{
    fmt::Debug,
    net::{SocketAddr, SocketAddrV4},
    ops::DerefMut,
};

use atm0s_sdn_identity::NodeId;
use quinn::{udp::EcnCodepoint, AsyncUdpSocket};

use crate::VirtualSocketPkt;

use super::{async_queue::AsyncQueue, internal::VirtualNetInternal, VirtualNetError};

pub struct VirtualUdpSocket {
    local_port: u16,
    internal: VirtualNetInternal,
    queue: AsyncQueue<VirtualSocketPkt>,
}

impl VirtualUdpSocket {
    pub(crate) fn new(internal: VirtualNetInternal, port: u16, buffer_size: usize) -> Result<Self, VirtualNetError> {
        let (queue, local_port) = internal.register_socket(port, buffer_size)?;
        Ok(Self { internal, queue, local_port })
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    pub fn send_to_node(&self, node: NodeId, port: u16, payload: &[u8], ecn: Option<u8>) -> Result<(), VirtualNetError> {
        self.internal.send_to_node(self.local_port, node, port, payload, ecn)
    }

    pub fn send_to(&self, dest: SocketAddrV4, payload: &[u8], ecn: Option<u8>) -> Result<(), VirtualNetError> {
        self.internal.send_to(self.local_port, dest, payload, ecn)
    }

    pub fn try_recv_from(&self) -> Option<VirtualSocketPkt> {
        self.queue.try_pop()
    }

    pub async fn recv_from(&self) -> Option<VirtualSocketPkt> {
        self.queue.recv().await
    }
}

impl Debug for VirtualUdpSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualUdpSocket").field("local_port", &self.local_port).finish()
    }
}

impl AsyncUdpSocket for VirtualUdpSocket {
    fn poll_send(&self, _state: &quinn::udp::UdpState, _cx: &mut std::task::Context, transmits: &[quinn::udp::Transmit]) -> std::task::Poll<Result<usize, std::io::Error>> {
        for transmit in transmits {
            let res = match transmit.destination {
                SocketAddr::V4(addr) => self.internal.send_to(self.local_port, addr, &transmit.contents, transmit.ecn.map(|x| x as u8)),
                _ => return std::task::Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Only IPv4 supported"))),
            };
            if res.is_err() {
                break;
            }
        }
        std::task::Poll::Ready(Ok(transmits.len()))
    }

    fn poll_recv(&self, cx: &mut std::task::Context, bufs: &mut [std::io::IoSliceMut<'_>], meta: &mut [quinn::udp::RecvMeta]) -> std::task::Poll<std::io::Result<usize>> {
        match self.queue.poll_pop(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(pkt)) => {
                let len = pkt.payload.len();
                bufs[0].deref_mut()[0..len].copy_from_slice(&pkt.payload);
                meta[0] = quinn::udp::RecvMeta {
                    addr: SocketAddr::V4(pkt.src),
                    len,
                    stride: len,
                    ecn: pkt.ecn.map(|x| EcnCodepoint::from_bits(x).expect("Invalid ECN codepoint")),
                    dst_ip: None,
                };
                std::task::Poll::Ready(Ok(1))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "Socket closed"))),
        }
    }

    fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        Ok(SocketAddr::V4(SocketAddrV4::new(self.internal.local_node().into(), self.local_port)))
    }
}

impl Drop for VirtualUdpSocket {
    fn drop(&mut self) {
        self.internal.unregister_socket(self.local_port);
    }
}

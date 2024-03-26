use std::{
    fmt::Debug,
    io::IoSliceMut,
    net::{SocketAddr, SocketAddrV4},
    ops::DerefMut,
    sync::Mutex,
    task::{Context, Poll},
};

use quinn::{
    udp::{EcnCodepoint, RecvMeta, Transmit, UdpState},
    AsyncUdpSocket,
};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedSender};

use super::{NetworkPkt, OutEvent};

pub struct VirtualUdpSocket {
    node_id: u32,
    port: u16,
    addr: SocketAddr,
    rx: Mutex<Receiver<NetworkPkt>>,
    tx: Sender<OutEvent>,
    close_socket_tx: UnboundedSender<u16>,
}

impl VirtualUdpSocket {
    pub fn new(node_id: u32, port: u16, tx: Sender<OutEvent>, rx: Receiver<NetworkPkt>, close_socket_tx: UnboundedSender<u16>) -> Self {
        Self {
            node_id,
            port,
            addr: SocketAddr::V4(SocketAddrV4::new(node_id.into(), port)),
            rx: Mutex::new(rx),
            tx,
            close_socket_tx,
        }
    }
}

impl Debug for VirtualUdpSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualUdpSocket").finish()
    }
}

impl AsyncUdpSocket for VirtualUdpSocket {
    fn poll_send(&self, _state: &UdpState, _cx: &mut Context, transmits: &[Transmit]) -> Poll<Result<usize, std::io::Error>> {
        let mut sent = 0;
        for transmit in transmits {
            match transmit.destination {
                SocketAddr::V4(addr) => {
                    let pkt = NetworkPkt {
                        local_port: self.port,
                        remote: u32::from_be_bytes(addr.ip().octets()),
                        remote_port: addr.port(),
                        data: transmit.contents.to_vec(),
                        meta: transmit.ecn.map(|x| x as u8).unwrap_or(0),
                    };
                    log::debug!("{} sending {} bytes to {}", self.addr, pkt.data.len(), addr);
                    if self.tx.try_send(OutEvent::Pkt(pkt)).is_ok() {
                        sent += 1;
                    }
                }
                _ => {
                    sent += 1;
                }
            }
        }
        std::task::Poll::Ready(Ok(sent))
    }

    fn poll_recv(&self, cx: &mut Context, bufs: &mut [IoSliceMut<'_>], meta: &mut [RecvMeta]) -> Poll<std::io::Result<usize>> {
        let mut rx = self.rx.lock().expect("should lock rx");
        match rx.poll_recv(cx) {
            std::task::Poll::Pending => std::task::Poll::Pending,
            std::task::Poll::Ready(Some(pkt)) => {
                let len = pkt.data.len();
                if len <= bufs[0].len() {
                    let addr = SocketAddr::V4(SocketAddrV4::new(pkt.remote.into(), pkt.remote_port));
                    log::debug!("{} received {} bytes from {}", self.addr, len, addr);
                    bufs[0].deref_mut()[0..len].copy_from_slice(&pkt.data);
                    meta[0] = quinn::udp::RecvMeta {
                        addr,
                        len,
                        stride: len,
                        ecn: if pkt.meta == 0 {
                            None
                        } else {
                            EcnCodepoint::from_bits(pkt.meta)
                        },
                        dst_ip: None,
                    };
                    std::task::Poll::Ready(Ok(1))
                } else {
                    log::warn!("Buffer too small for packet {} vs {}, dropping", len, bufs[0].len());
                    std::task::Poll::Pending
                }
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "Socket closed"))),
        }
    }

    fn local_addr(&self) -> std::io::Result<SocketAddr> {
        Ok(self.addr)
    }
}

impl Drop for VirtualUdpSocket {
    fn drop(&mut self) {
        self.close_socket_tx.send(self.port).expect("Should send success");
    }
}

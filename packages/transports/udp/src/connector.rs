use std::{
    net::SocketAddr,
    os::fd::{AsRawFd, FromRawFd},
    sync::{atomic::AtomicU64, Arc},
};

use async_std::channel::Sender;
use bluesea_identity::{ConnId, NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use network::transport::{AsyncConnectionAcceptor, ConnectionRejectReason, OutgoingConnectionError, TransportConnectingOutgoing, TransportConnector, TransportEvent};
use utils::{error_handle::ErrorUtils, Timer};

use crate::{
    handshake::{outgoing_handshake, OutgoingHandshakeError},
    receiver::UdpClientConnectionReceiver,
    sender::UdpClientConnectionSender,
    UDP_PROTOCOL_ID,
};

pub struct UdpConnector {
    local_node_id: NodeId,
    local_addr_builder: Arc<NodeAddrBuilder>,
    conn_id_seed: AtomicU64,
    tx: Sender<TransportEvent>,
    timer: Arc<dyn Timer>,
}

impl UdpConnector {
    pub fn new(local_node_id: NodeId, local_addr_builder: Arc<NodeAddrBuilder>, tx: Sender<TransportEvent>, timer: Arc<dyn Timer>) -> Self {
        Self {
            local_node_id,
            local_addr_builder,
            conn_id_seed: AtomicU64::new(0),
            tx,
            timer,
        }
    }

    /// Extracts a `SocketAddr` from a given `Multiaddr`.
    ///
    /// Fails if the given `Multiaddr` does not begin with an IP
    /// protocol encapsulating a UDP port.
    fn multiaddr_to_socketaddr(mut addr: NodeAddr) -> Result<SocketAddr, ()> {
        // "Pop" the IP address and UDP port from the end of the address,
        // ignoring a `/p2p/...` suffix as well as any prefix of possibly
        // outer protocols, if present.
        let mut port = None;
        while let Some(proto) = addr.pop() {
            match proto {
                Protocol::Ip4(ipv4) => match port {
                    Some(port) => return Ok(SocketAddr::new(ipv4.into(), port)),
                    None => return Err(()),
                },
                Protocol::Ip6(ipv6) => match port {
                    Some(port) => return Ok(SocketAddr::new(ipv6.into(), port)),
                    None => return Err(()),
                },
                Protocol::Udp(portnum) => match port {
                    Some(_) => return Err(()),
                    None => port = Some(portnum),
                },
                Protocol::P2p(_) => {}
                _ => return Err(()),
            }
        }
        Err(())
    }
}

impl TransportConnector for UdpConnector {
    fn connect_to(&self, node_id: NodeId, dest: NodeAddr) -> Result<TransportConnectingOutgoing, OutgoingConnectionError> {
        let remote_addr = Self::multiaddr_to_socketaddr(dest.clone()).map_err(|_| OutgoingConnectionError::UnsupportedProtocol)?;
        let conn_id_uuid = self.conn_id_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let conn_id = ConnId::from_out(UDP_PROTOCOL_ID, conn_id_uuid);
        let local_node_id = self.local_node_id;
        let local_node_addr = self.local_addr_builder.addr();
        let tx = self.tx.clone();
        let timer = self.timer.clone();

        async_std::task::spawn(async move {
            if let Err(e) = wait_accept(node_id, conn_id, &tx).await {
                log::error!("Outgoing handshake error {:?}", e);
                tx.send(TransportEvent::OutgoingError {
                    node_id,
                    conn_id,
                    err: OutgoingConnectionError::BehaviorRejected(ConnectionRejectReason::Custom("LocalReject".to_string())),
                })
                .await
                .print_error("Should send Outgoing Error");
                return;
            }

            let socket = Arc::new(std::net::UdpSocket::bind("0.0.0.0:0").expect("Should bind a address"));
            socket.connect(remote_addr).print_error("Should connect to remote addr");
            let async_socket = unsafe { Arc::new(async_std::net::UdpSocket::from_raw_fd(socket.as_raw_fd())) };

            match outgoing_handshake(&async_socket, local_node_id, local_node_addr, node_id).await {
                Ok(_) => {
                    let close_state = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let close_notify = Arc::new(async_notify::Notify::new());
                    let sender = Arc::new(UdpClientConnectionSender::new(node_id, dest.clone(), conn_id, socket, close_state.clone(), close_notify.clone()));
                    let receiver = Box::new(UdpClientConnectionReceiver::new(async_socket, conn_id, node_id, dest, timer, close_state, close_notify));
                    tx.send(TransportEvent::Outgoing(sender, receiver)).await.print_error("Should send incoming event");
                }
                Err(e) => {
                    log::error!("{:?}", e);
                    tx.send(TransportEvent::OutgoingError {
                        node_id,
                        conn_id,
                        err: match e {
                            OutgoingHandshakeError::SocketError => OutgoingConnectionError::DestinationNotFound,
                            OutgoingHandshakeError::Timeout => OutgoingConnectionError::AuthenticationError,
                            OutgoingHandshakeError::WrongMsg => OutgoingConnectionError::AuthenticationError,
                            OutgoingHandshakeError::InternalError => OutgoingConnectionError::AuthenticationError,
                            OutgoingHandshakeError::Rejected => OutgoingConnectionError::AuthenticationError,
                            OutgoingHandshakeError::DestinationError => OutgoingConnectionError::DestinationNotFound,
                            OutgoingHandshakeError::AuthenticationError => OutgoingConnectionError::AuthenticationError,
                        },
                    })
                    .await
                    .print_error("Should send outgoing error");
                }
            }
        });

        Ok(TransportConnectingOutgoing { conn_id })
    }
}

async fn wait_accept(remote_node: NodeId, conn_id: ConnId, internal_tx: &Sender<TransportEvent>) -> Result<(), OutgoingHandshakeError> {
    log::info!("[UdpConnector] connect to {} send local check", remote_node);
    let (connection_acceptor, recv) = AsyncConnectionAcceptor::new();
    internal_tx
        .send(TransportEvent::OutgoingRequest(remote_node, conn_id, connection_acceptor))
        .await
        .map_err(|_| OutgoingHandshakeError::InternalError)?;
    log::info!("[UdpConnector] connect to {} wait local accept", remote_node);
    if let Err(e) = recv.recv().await.map_err(|_| OutgoingHandshakeError::InternalError)? {
        log::error!("Connection rejected {:?}", e);
        return Err(OutgoingHandshakeError::Rejected);
    }
    Ok(())
}

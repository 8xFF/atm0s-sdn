use crate::connection::{TcpConnectionReceiver, TcpConnectionSender, BUFFER_LEN};
use crate::handshake::{outgoing_handshake, OutgoingHandshakeError};
use crate::OUTGOING_POSTFIX;
use async_std::channel::{bounded, unbounded, Sender};
use async_std::net::{Shutdown, TcpStream};
use bluesea_identity::{PeerAddr, PeerAddrBuilder, PeerId, Protocol};
use futures_util::{AsyncReadExt, AsyncWriteExt};
use network::transport::{
    AsyncConnectionAcceptor, ConnectionRejectReason, OutgoingConnectionError, TransportConnector,
    TransportEvent, TransportPendingOutgoing,
};
use serde::{de::DeserializeOwned, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub struct TcpConnector<MSG> {
    pub(crate) seed: AtomicU32,
    pub(crate) peer_id: PeerId,
    pub(crate) peer_addr_builder: Arc<PeerAddrBuilder>,
    pub(crate) internal_tx: Sender<TransportEvent<MSG>>,
}

impl<MSG> TcpConnector<MSG> {
    /// Extracts a `SocketAddr` from a given `Multiaddr`.
    ///
    /// Fails if the given `Multiaddr` does not begin with an IP
    /// protocol encapsulating a TCP port.
    fn multiaddr_to_socketaddr(mut addr: PeerAddr) -> Result<SocketAddr, ()> {
        // "Pop" the IP address and TCP port from the end of the address,
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
                Protocol::Tcp(portnum) => match port {
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

async fn wait_accept<MSG>(
    remote_peer: PeerId,
    conn_id: u32,
    internal_tx: &Sender<TransportEvent<MSG>>,
) -> Result<(), OutgoingHandshakeError> {
    log::info!("[TcpConnector] connect to {} send local check", remote_peer);
    let (connection_acceptor, recv) = AsyncConnectionAcceptor::new();
    internal_tx
        .send(TransportEvent::OutgoingRequest(
            remote_peer,
            conn_id,
            connection_acceptor,
        ))
        .await
        .map_err(|_| OutgoingHandshakeError::InternalError)?;
    log::info!(
        "[TcpConnector] connect to {} wait local accept",
        remote_peer
    );
    if let Err(e) = recv
        .recv()
        .await
        .map_err(|_| OutgoingHandshakeError::InternalError)?
    {
        return Err(OutgoingHandshakeError::Rejected);
    }
    Ok(())
}

impl<MSG> TransportConnector for TcpConnector<MSG>
where
    MSG: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn connect_to(
        &self,
        remote_peer_id: PeerId,
        remote_peer_addr: PeerAddr,
    ) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        log::info!("[TcpConnector] connect to peer {}", remote_peer_addr);
        let peer_id = self.peer_id;
        let peer_addr = self.peer_addr_builder.addr();
        let remote_addr = Self::multiaddr_to_socketaddr(remote_peer_addr.clone())
            .map_err(|_| OutgoingConnectionError::UnsupportedProtocol)?;
        let conn_id = self.seed.load(Ordering::Relaxed) * 100 + OUTGOING_POSTFIX;
        let internal_tx = self.internal_tx.clone();
        self.seed.fetch_add(1, Ordering::Relaxed);
        async_std::task::spawn(async move {
            if let Err(e) = wait_accept(remote_peer_id, conn_id, &internal_tx).await {
                internal_tx
                    .send(TransportEvent::OutgoingError {
                        peer_id: remote_peer_id,
                        connection_id: conn_id,
                        err: OutgoingConnectionError::BehaviorRejected(
                            ConnectionRejectReason::Custom("LocalReject".to_string()),
                        ),
                    })
                    .await;
                return;
            }

            match TcpStream::connect(remote_addr).await {
                Ok(mut socket) => {
                    match outgoing_handshake::<MSG>(
                        remote_peer_id,
                        peer_id,
                        peer_addr,
                        &mut socket,
                        conn_id,
                        &internal_tx,
                    )
                    .await
                    {
                        Ok(_) => {
                            let connection_sender = Arc::new(TcpConnectionSender::new(
                                remote_peer_id,
                                remote_peer_addr.clone(),
                                conn_id,
                                1000,
                                socket.clone(),
                            ));
                            let connection_receiver = Box::new(TcpConnectionReceiver {
                                remote_peer_id,
                                remote_addr: remote_peer_addr,
                                conn_id,
                                socket,
                                buf: [0; BUFFER_LEN],
                            });
                            internal_tx
                                .send(TransportEvent::Outgoing(
                                    connection_sender,
                                    connection_receiver,
                                ))
                                .await;
                        }
                        Err(err) => {
                            socket.shutdown(Shutdown::Both);
                            internal_tx
                                .send(TransportEvent::OutgoingError {
                                    peer_id: remote_peer_id,
                                    connection_id: conn_id,
                                    err: match err {
                                        OutgoingHandshakeError::SocketError => {
                                            OutgoingConnectionError::DestinationNotFound
                                        }
                                        OutgoingHandshakeError::Timeout => {
                                            OutgoingConnectionError::AuthenticationError
                                        }
                                        OutgoingHandshakeError::WrongMsg => {
                                            OutgoingConnectionError::AuthenticationError
                                        }
                                        OutgoingHandshakeError::InternalError => {
                                            OutgoingConnectionError::AuthenticationError
                                        }
                                        OutgoingHandshakeError::Rejected => {
                                            OutgoingConnectionError::AuthenticationError
                                        }
                                        OutgoingHandshakeError::NetError => {
                                            OutgoingConnectionError::DestinationNotFound
                                        }
                                    },
                                })
                                .await;
                        }
                    }
                }
                Err(err) => {
                    internal_tx
                        .send(TransportEvent::OutgoingError {
                            peer_id: remote_peer_id,
                            connection_id: conn_id,
                            err: OutgoingConnectionError::DestinationNotFound,
                        })
                        .await;
                }
            }
        });
        Ok(TransportPendingOutgoing {
            connection_id: conn_id,
        })
    }
}

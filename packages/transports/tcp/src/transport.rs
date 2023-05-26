use crate::connection::{recv_tcp_stream, TcpConnectionReceiver, TcpConnectionSender, BUFFER_LEN};
use crate::connector::TcpConnector;
use crate::handshake::{incoming_handshake, IncomingHandshakeError};
use crate::msg::TcpMsg;
use crate::INCOMING_POSTFIX;
use async_std::channel::{bounded, unbounded, Receiver, Sender};
use async_std::net::{Shutdown, TcpListener};
use bluesea_identity::{PeerAddrBuilder, PeerId, Protocol};
use futures_util::{select, AsyncReadExt, AsyncWriteExt, FutureExt};
use network::transport::{AsyncConnectionAcceptor, Transport, TransportConnector, TransportEvent};
use serde::{de::DeserializeOwned, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Duration;
use utils::{SystemTimer, Timer};

pub struct TcpTransport<MSG> {
    peer_id: PeerId,
    peer_addr_builder: Arc<PeerAddrBuilder>,
    listener: TcpListener,
    internal_tx: Sender<TransportEvent<MSG>>,
    internal_rx: Receiver<TransportEvent<MSG>>,
    seed: u32,
    connector: Arc<TcpConnector<MSG>>,
    timer: Arc<dyn Timer>,
}

impl<MSG> TcpTransport<MSG> {
    pub async fn new(peer_id: PeerId, port: u16, peer_addr_builder: Arc<PeerAddrBuilder>) -> Self {
        let (internal_tx, internal_rx) = unbounded();
        let addr_str = format!("0.0.0.0:{}", port);
        let addr: SocketAddr = addr_str.as_str().parse().unwrap();
        let listener = TcpListener::bind(addr).await.unwrap();

        //TODO get dynamic ip address
        peer_addr_builder.add_protocol(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)));
        if port != 0 {
            peer_addr_builder.add_protocol(Protocol::Tcp(port));
        } else {
            if let Ok(addr) = listener.local_addr() {
                peer_addr_builder.add_protocol(Protocol::Tcp(addr.port()));
            }
        }

        Self {
            peer_id,
            peer_addr_builder: peer_addr_builder.clone(),
            listener,
            internal_tx: internal_tx.clone(),
            internal_rx,
            seed: 0,
            connector: Arc::new(TcpConnector {
                seed: Default::default(),
                peer_id,
                peer_addr_builder,
                internal_tx,
                timer: Arc::new(SystemTimer()),
            }),
            timer: Arc::new(SystemTimer()),
        }
    }
}

#[async_trait::async_trait]
impl<MSG> Transport<MSG> for TcpTransport<MSG>
where
    MSG: Send + Sync + Serialize + DeserializeOwned + 'static,
{
    fn connector(&self) -> Arc<dyn TransportConnector> {
        self.connector.clone()
    }

    async fn recv(&mut self) -> Result<TransportEvent<MSG>, ()> {
        loop {
            select! {
                e = self.listener.accept().fuse() => match e {
                    Ok((mut socket, addr)) => {
                        log::info!("[TcpTransport] incoming connect from {}", addr);
                        let internal_tx = self.internal_tx.clone();
                        let timer = self.timer.clone();
                        let peer_id = self.peer_id;
                        let peer_addr = self.peer_addr_builder.addr();
                        let conn_id = self.seed * 100 + INCOMING_POSTFIX;
                        self.seed += 1;

                        async_std::task::spawn(async move {
                            match incoming_handshake::<MSG>(peer_id, peer_addr, &mut socket, conn_id, &internal_tx).await {
                                Ok((remote_peer_id, remote_addr)) => {
                                    let (connection_sender, reliable_sender) = TcpConnectionSender::new(
                                        peer_id,
                                        remote_peer_id,
                                        remote_addr.clone(),
                                        conn_id,
                                        1000,
                                        socket.clone(),
                                        timer.clone(),
                                    );
                                    let connection_receiver = Box::new(TcpConnectionReceiver {
                                        peer_id,
                                        remote_peer_id,
                                        remote_addr,
                                        conn_id,
                                        socket,
                                        buf: [0; BUFFER_LEN],
                                        timer,
                                        reliable_sender,
                                    });
                                    internal_tx.send(TransportEvent::Incoming(
                                        Arc::new(connection_sender),
                                        connection_receiver,
                                    )).await;
                                }
                                Err(_) => {
                                    if let Err(e) = socket.shutdown(Shutdown::Both) {
                                        log::error!("[TcpTransport] close handshake failed socket error {}", e);
                                    } else {
                                        log::info!("[TcpTransport] close handshake failed socket");
                                    }
                                }
                            }
                        });
                    }
                    Err(_) => {
                        break Err(());
                    }
                },
                e = self.internal_rx.recv().fuse() => {
                    break e.map_err(|_| ());
                }
            }
        }
    }
}

use crate::connection::{TcpConnectionReceiver, TcpConnectionSender};
use crate::connector::TcpConnector;
use crate::handshake::incoming_handshake;
use crate::msg::TcpMsg;
use async_bincode::futures::AsyncBincodeStream;
use async_std::channel::{unbounded, Receiver, Sender};
use async_std::net::TcpListener;
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use atm0s_sdn_network::secure::DataSecure;
use atm0s_sdn_network::transport::{Transport, TransportConnector, TransportEvent};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use atm0s_sdn_utils::{SystemTimer, Timer};
use futures_util::FutureExt;
use local_ip_address::local_ip;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr};
use std::sync::Arc;

pub struct TcpTransport {
    secure: Arc<dyn DataSecure>,
    node_id: NodeId,
    listener: TcpListener,
    internal_tx: Sender<TransportEvent>,
    internal_rx: Receiver<TransportEvent>,
    seed: u64,
    connector: TcpConnector,
    timer: Arc<dyn Timer>,
}

impl TcpTransport {
    pub async fn prepare(port: u16, node_addr_builder: &mut NodeAddrBuilder) -> TcpListener {
        let listener = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)).await.unwrap();

        match local_ip() {
            Ok(ip) => {
                node_addr_builder.add_protocol(match ip {
                    IpAddr::V4(ip) => Protocol::Ip4(ip),
                    IpAddr::V6(ip) => Protocol::Ip6(ip),
                });
            }
            Err(e) => {
                log::error!("[TcpTransport] get local ip address error {:?} => fallback to loopback 127.0.0.1", e);
                node_addr_builder.add_protocol(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)));
            }
        }

        if port != 0 {
            node_addr_builder.add_protocol(Protocol::Tcp(port));
        } else if let Ok(addr) = listener.local_addr() {
            node_addr_builder.add_protocol(Protocol::Tcp(addr.port()));
        }

        listener
    }

    pub fn new(node_addr: NodeAddr, listener: TcpListener, secure: Arc<dyn DataSecure>) -> Self {
        let node_id = node_addr.node_id();
        let (internal_tx, internal_rx) = unbounded();

        Self {
            secure: secure.clone(),
            node_id,
            listener,
            internal_tx: internal_tx.clone(),
            internal_rx,
            seed: 0,
            connector: TcpConnector {
                secure,
                conn_id_seed: 0,
                node_addr,
                pending_outgoing: HashMap::new(),
                node_id,
                internal_tx,
                timer: Arc::new(SystemTimer()),
            },
            timer: Arc::new(SystemTimer()),
        }
    }
}

#[async_trait::async_trait]
impl Transport for TcpTransport {
    fn connector(&mut self) -> &mut dyn TransportConnector {
        &mut self.connector
    }

    async fn recv(&mut self) -> Result<TransportEvent, ()> {
        loop {
            futures_util::select! {
                e = self.listener.accept().fuse() => match e {
                    Ok((socket, addr)) => {
                        log::info!("[TcpTransport] incoming connect from {}", addr);
                        let internal_tx = self.internal_tx.clone();
                        let timer = self.timer.clone();
                        let node_id = self.node_id;
                        let conn_id = ConnId::from_in(1, self.seed);
                        let secure = self.secure.clone();
                        self.seed += 1;

                        async_std::task::spawn(async move {
                            let mut socket_read = AsyncBincodeStream::<_, TcpMsg, TcpMsg, _>::from(socket.clone()).for_async();
                            let socket_write = AsyncBincodeStream::<_, TcpMsg, TcpMsg, _>::from(socket.clone()).for_async();

                            match incoming_handshake(secure, node_id, &mut socket_read, conn_id, &internal_tx).await {
                                Ok((remote_node_id, remote_addr)) => {
                                    let (connection_sender, unreliable_sender) = TcpConnectionSender::new(
                                        node_id,
                                        remote_node_id,
                                        remote_addr.clone(),
                                        conn_id,
                                        1000,
                                        socket_write,
                                        timer.clone(),
                                    );
                                    let connection_receiver = Box::new(TcpConnectionReceiver {
                                        remote_node_id,
                                        remote_addr,
                                        conn_id,
                                        socket: socket_read,
                                        timer,
                                        unreliable_sender,
                                    });
                                    internal_tx.send(TransportEvent::Incoming(
                                        Arc::new(connection_sender),
                                        connection_receiver,
                                    )).await.print_error("Should send Incoming connection");
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

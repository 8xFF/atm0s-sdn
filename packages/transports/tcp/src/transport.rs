use crate::connection::{TcpConnectionSender, TcpConnectionReceiver};
use crate::connector::TcpConnector;
use crate::handshake::incoming_handshake;
use crate::msg::TcpMsg;
use async_bincode::futures::AsyncBincodeStream;
use async_std::channel::{unbounded, Receiver, Sender};
use async_std::net::TcpListener;
use bluesea_identity::{NodeAddrBuilder, NodeId, Protocol, ConnId};
use futures_util::FutureExt;
use network::transport::{Transport, TransportConnector, TransportEvent};
use std::net::{Ipv4Addr, SocketAddr, Shutdown};
use std::sync::Arc;
use utils::{SystemTimer, Timer};

pub struct TcpTransport {
    node_id: NodeId,
    node_addr_builder: Arc<NodeAddrBuilder>,
    listener: TcpListener,
    internal_tx: Sender<TransportEvent>,
    internal_rx: Receiver<TransportEvent>,
    seed: u64,
    connector: Arc<TcpConnector>,
    timer: Arc<dyn Timer>,
}

impl TcpTransport {
    pub async fn new(node_id: NodeId, port: u16, node_addr_builder: Arc<NodeAddrBuilder>) -> Self {
        let (internal_tx, internal_rx) = unbounded();
        let addr_str = format!("0.0.0.0:{}", port);
        let addr: SocketAddr = addr_str.as_str().parse().unwrap();
        let listener = TcpListener::bind(addr).await.unwrap();

        //TODO get dynamic ip address
        node_addr_builder.add_protocol(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)));
        if port != 0 {
            node_addr_builder.add_protocol(Protocol::Tcp(port));
        } else {
            if let Ok(addr) = listener.local_addr() {
                node_addr_builder.add_protocol(Protocol::Tcp(addr.port()));
            }
        }

        Self {
            node_id,
            node_addr_builder: node_addr_builder.clone(),
            listener,
            internal_tx: internal_tx.clone(),
            internal_rx,
            seed: 0,
            connector: Arc::new(TcpConnector {
                seed: Default::default(),
                node_id,
                node_addr_builder,
                internal_tx,
                timer: Arc::new(SystemTimer()),
            }),
            timer: Arc::new(SystemTimer()),
        }
    }
}

#[async_trait::async_trait]
impl Transport for TcpTransport {
    fn connector(&self) -> Arc<dyn TransportConnector> {
        self.connector.clone()
    }

    async fn recv(&mut self) -> Result<TransportEvent, ()> {
        loop {
            futures_util::select! {
                e = self.listener.accept().fuse() => match e {
                    Ok((mut socket, addr)) => {
                        log::info!("[TcpTransport] incoming connect from {}", addr);
                        let internal_tx = self.internal_tx.clone();
                        let timer = self.timer.clone();
                        let node_id = self.node_id;
                        let node_addr = self.node_addr_builder.addr();
                        let conn_id = ConnId::from_in(1, self.seed);
                        self.seed += 1;

                        async_std::task::spawn(async move {
                            let mut socket_read = AsyncBincodeStream::<_, TcpMsg, TcpMsg, _>::from(socket.clone()).for_async();
                            let mut socket_write = AsyncBincodeStream::<_, TcpMsg, TcpMsg, _>::from(socket.clone()).for_async();

                            match incoming_handshake(node_id, node_addr, &mut socket_read, conn_id, &internal_tx).await {
                                Ok((remote_node_id, remote_addr)) => {
                                    let (connection_sender, reliable_sender) = TcpConnectionSender::new(
                                        node_id,
                                        remote_node_id,
                                        remote_addr.clone(),
                                        conn_id,
                                        1000,
                                        socket_write,
                                        timer.clone(),
                                    );
                                    let connection_receiver = Box::new(TcpConnectionReceiver {
                                        node_id,
                                        remote_node_id,
                                        remote_addr,
                                        conn_id,
                                        socket: socket_read,
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

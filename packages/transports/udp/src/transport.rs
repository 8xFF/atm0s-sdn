use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    os::fd::{AsRawFd, FromRawFd},
    sync::Arc,
};

use async_std::channel::{Receiver, Sender};
use bluesea_identity::{ConnId, NodeAddrBuilder, NodeId, Protocol};
use network::transport::{Transport, TransportConnector, TransportEvent};
use std::net::UdpSocket;
use utils::{error_handle::ErrorUtils, SystemTimer, Timer};

use crate::{connector::UdpConnector, handshake::incoming_handshake, receiver::UdpServerConnectionReceiver, sender::UdpServerConnectionSender, UDP_PROTOCOL_ID};

pub struct UdpTransport {
    rx: Receiver<TransportEvent>,
    connector: Arc<dyn TransportConnector>,
}

impl UdpTransport {
    pub async fn new(node_id: NodeId, port: u16, node_addr_builder: Arc<NodeAddrBuilder>) -> Self {
        let (tx, rx) = async_std::channel::bounded(1024);
        let addr_str = format!("0.0.0.0:{}", port);
        let addr: SocketAddr = addr_str.as_str().parse().expect("Should parse ip address");
        let socket = Arc::new(UdpSocket::bind(addr).expect("Should bind udp socket"));

        log::info!("[UdpTransport] Listening on port {}", socket.local_addr().unwrap().port());

        //TODO get dynamic ip address
        node_addr_builder.add_protocol(Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)));
        if port != 0 {
            node_addr_builder.add_protocol(Protocol::Udp(port));
        } else if let Ok(addr) = socket.local_addr() {
            node_addr_builder.add_protocol(Protocol::Udp(addr.port()));
        }

        let timer = Arc::new(SystemTimer());
        let connector = Arc::new(UdpConnector::new(node_id, node_addr_builder.clone(), tx.clone(), timer.clone()));

        async_std::task::spawn(async move {
            let mut last_clear_timeout_ms = 0;
            let mut conn_id_seed = 0;
            let mut connection: HashMap<SocketAddr, (Sender<([u8; 1500], usize)>, u64)> = HashMap::new();
            let async_socket = unsafe { Arc::new(async_std::net::UdpSocket::from_raw_fd(socket.as_raw_fd())) };
            loop {
                let mut buf = [0u8; 1500];
                if let Ok((size, addr)) = async_socket.recv_from(&mut buf).await {
                    let current_ms = timer.now_ms();
                    if let Some(msg_tx) = connection.get_mut(&addr) {
                        msg_tx.0.try_send((buf, size)).expect("should forward to receiver");
                        msg_tx.1 = current_ms;
                    } else {
                        log::info!("[UdpTransport] on new connection from {}", addr);
                        conn_id_seed += 1;
                        let conn_id = ConnId::from_in(UDP_PROTOCOL_ID, conn_id_seed);
                        let (msg_tx, msg_rx) = async_std::channel::bounded(1024);
                        msg_tx.try_send((buf, size)).expect("should forward to receiver");
                        connection.insert(addr, (msg_tx, current_ms));
                        let socket = socket.clone();
                        let async_socket = async_socket.clone();
                        let tx = tx.clone();
                        let timer = timer.clone();
                        async_std::task::spawn(async move {
                            match incoming_handshake(node_id, &tx, &msg_rx, conn_id, addr, &async_socket).await {
                                Ok((remote_node_id, remote_node_addr)) => {
                                    let close_state = Arc::new(std::sync::atomic::AtomicBool::new(false));
                                    let close_notify = Arc::new(async_notify::Notify::new());
                                    let sender = Arc::new(UdpServerConnectionSender::new(
                                        remote_node_id,
                                        remote_node_addr.clone(),
                                        conn_id,
                                        socket,
                                        addr,
                                        close_state.clone(),
                                        close_notify.clone(),
                                    ));
                                    let receiver = Box::new(UdpServerConnectionReceiver::new(
                                        async_socket.clone(),
                                        addr,
                                        msg_rx,
                                        conn_id,
                                        remote_node_id,
                                        remote_node_addr,
                                        timer.clone(),
                                        close_state,
                                        close_notify,
                                    ));
                                    log::info!("[UdpTransport] on connection success handshake from {}", addr);
                                    tx.send(TransportEvent::Incoming(sender, receiver)).await.print_error("Should send incoming event");
                                }
                                Err(e) => {
                                    log::error!("[UdpTransport] process incoming handshake from {} error {:?}", addr, e);
                                }
                            }
                        });
                    }

                    if last_clear_timeout_ms + 1000 < current_ms {
                        let mut remove_list = Vec::new();
                        for (addr, (_, last_ms)) in connection.iter() {
                            if last_ms + 10000 < current_ms {
                                remove_list.push(addr.clone());
                            }
                        }

                        for addr in remove_list {
                            connection.remove(&addr);
                        }

                        last_clear_timeout_ms = current_ms;
                    }
                }
            }
        });

        Self { rx, connector }
    }
}

#[async_trait::async_trait]
impl Transport for UdpTransport {
    fn connector(&self) -> Arc<dyn TransportConnector> {
        self.connector.clone()
    }

    async fn recv(&mut self) -> Result<TransportEvent, ()> {
        self.rx.recv().await.map_err(|_| ())
    }
}

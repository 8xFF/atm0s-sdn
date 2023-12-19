use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    os::fd::{AsRawFd, FromRawFd},
    sync::Arc,
};

use async_std::channel::Sender;
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId, Protocol};
use atm0s_sdn_network::transport::{OutgoingConnectionError, TransportConnector, TransportEvent};
use atm0s_sdn_utils::{error_handle::ErrorUtils, Timer};

use crate::{
    handshake::{outgoing_handshake, OutgoingHandshakeError},
    receiver::UdpClientConnectionReceiver,
    sender::UdpClientConnectionSender,
    UDP_PROTOCOL_ID,
};

pub struct UdpConnector {
    local_node_id: NodeId,
    local_addr: NodeAddr,
    conn_id_seed: u64,
    tx: Sender<TransportEvent>,
    timer: Arc<dyn Timer>,
    pending_outgoing: HashMap<ConnId, (NodeId, NodeAddr, SocketAddr)>,
}

impl UdpConnector {
    pub fn new(local_node_id: NodeId, local_addr: NodeAddr, tx: Sender<TransportEvent>, timer: Arc<dyn Timer>) -> Self {
        Self {
            local_node_id,
            local_addr,
            conn_id_seed: 0,
            tx,
            timer,
            pending_outgoing: HashMap::new(),
        }
    }
}

impl TransportConnector for UdpConnector {
    fn create_pending_outgoing(&mut self, dest: NodeAddr) -> Vec<ConnId> {
        let mut res = vec![];
        let mut ip_v4 = None;
        for proto in dest.multiaddr().iter() {
            match proto {
                Protocol::Ip4(ip) => {
                    ip_v4 = Some(ip);
                }
                Protocol::Udp(portnum) => match &ip_v4 {
                    Some(ip) => {
                        let uuid = self.conn_id_seed;
                        self.conn_id_seed += 1;
                        let conn_id = ConnId::from_out(UDP_PROTOCOL_ID, uuid);
                        res.push(conn_id);
                        self.pending_outgoing.insert(conn_id, (dest.node_id(), dest.clone(), SocketAddr::new(ip.clone().into(), portnum)));
                    }
                    None => {}
                },
                Protocol::Memory(_) => {}
                _ => {}
            }
        }
        res
    }

    fn continue_pending_outgoing(&mut self, conn_id: ConnId) {
        if let Some((node_id, node_addr, remote_addr)) = self.pending_outgoing.remove(&conn_id) {
            let local_node_id = self.local_node_id;
            let local_node_addr = self.local_addr.clone();
            let tx = self.tx.clone();
            let timer = self.timer.clone();

            async_std::task::spawn(async move {
                let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).expect("Should create socket");
                let address: SocketAddr = "0.0.0.0:0".parse().expect("Should parse socket_addr");
                socket.bind(&address.into()).expect("Should bind address");
                socket.set_recv_buffer_size(1024 * 1024).expect("Should set recv buffer size");
                socket.set_send_buffer_size(1024 * 1024).expect("Should set recv buffer size");
                let socket: UdpSocket = socket.into();
                let socket = Arc::new(socket);
                socket.connect(remote_addr).print_error("Should connect to remote addr");

                let async_socket = unsafe { Arc::new(async_std::net::UdpSocket::from_raw_fd(socket.as_raw_fd())) };

                match outgoing_handshake(&async_socket, local_node_id, local_node_addr, node_id).await {
                    Ok(_) => {
                        let close_state = Arc::new(std::sync::atomic::AtomicBool::new(false));
                        let close_notify = Arc::new(async_notify::Notify::new());
                        let sender = Arc::new(UdpClientConnectionSender::new(node_id, node_addr.clone(), conn_id, socket, close_state.clone(), close_notify.clone()));
                        let receiver = Box::new(UdpClientConnectionReceiver::new(async_socket, conn_id, node_id, node_addr, timer, close_state, close_notify));
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
                                OutgoingHandshakeError::Rejected => OutgoingConnectionError::AuthenticationError,
                                OutgoingHandshakeError::AuthenticationError => OutgoingConnectionError::AuthenticationError,
                            },
                        })
                        .await
                        .print_error("Should send outgoing error");
                    }
                }
            });
        }
    }

    fn destroy_pending_outgoing(&mut self, conn_id: ConnId) {
        self.pending_outgoing.remove(&conn_id);
    }
}

use crate::connection::{VnetConnection, VnetConnectionReceiver, VnetConnectionSender};
use crate::listener::{VnetListener, VnetListenerEvent};
use async_std::channel::{unbounded, Sender};
use bluesea_identity::{PeerAddr, PeerId};
use network::transport::{
    AsyncConnectionAcceptor, ConnectionRejectReason, OutgoingConnectionError,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

struct Socket<MSG> {
    peer: PeerId,
    addr: PeerAddr,
    sender: Sender<VnetListenerEvent<MSG>>,
}

pub struct VnetEarth<MSG> {
    conn_id_seed: AtomicU32,
    ports: RwLock<HashMap<u64, Socket<MSG>>>,
}

impl<MSG> Default for VnetEarth<MSG> {
    fn default() -> Self {
        Self {
            conn_id_seed: Default::default(),
            ports: Default::default(),
        }
    }
}

impl<MSG> VnetEarth<MSG>
where
    MSG: Send + Sync + 'static,
{
    pub fn create_listener(&self, port: u64, peer: PeerId, addr: PeerAddr) -> VnetListener<MSG> {
        let (tx, rx) = unbounded();
        self.ports.write().insert(
            port,
            Socket {
                peer,
                addr,
                sender: tx,
            },
        );
        VnetListener { rx }
    }

    pub fn create_outgoing(&self, from_port: u64, to_peer: PeerId, to_port: u64) -> Option<u32> {
        assert_ne!(from_port, to_port);
        let ports = self.ports.read();
        let from_socket = ports.get(&from_port)?;
        let conn_id = self.conn_id_seed.fetch_add(1, Ordering::Relaxed);
        if let Some(to_socket) = ports.get(&to_port) {
            if to_socket.peer == to_peer {
                let (incoming_acceptor, mut incoming_acceptor_recv) =
                    AsyncConnectionAcceptor::new();
                let (outgoing_acceptor, mut outgoing_acceptor_recv) =
                    AsyncConnectionAcceptor::new();
                let from_socket_sender = from_socket.sender.clone();
                let from_socket_peer = from_socket.peer;
                let from_socket_addr = from_socket.addr.clone();
                let to_socket_sender = to_socket.sender.clone();
                let to_socket_peer = to_socket.peer;
                let to_socket_addr = to_socket.addr.clone();
                async_std::task::spawn(async move {
                    let (from_tx, from_rx) = unbounded();
                    let (to_tx, to_rx) = unbounded();

                    let outgoing_res = outgoing_acceptor_recv.recv().await;
                    let incoming_res = incoming_acceptor_recv.recv().await;
                    let err = match (outgoing_res, incoming_res) {
                        (Ok(Ok(())), Ok(Ok(()))) => None,
                        (Ok(Err(e)), _) => Some(e),
                        (_, Ok(Err(e))) => Some(e),
                        _ => Some(ConnectionRejectReason::Custom("ChannelError".to_string())),
                    };

                    if let Some(err) = err {
                        from_socket_sender.send_blocking(VnetListenerEvent::OutgoingErr(
                            to_peer,
                            conn_id,
                            OutgoingConnectionError::BehaviorRejected(err),
                        ));
                    } else {
                        from_socket_sender
                            .send_blocking(VnetListenerEvent::Outgoing((
                                Arc::new(VnetConnectionSender {
                                    remote_peer_id: to_socket_peer,
                                    conn_id,
                                    remote_addr: to_socket_addr.clone(),
                                    sender: from_tx.clone(),
                                    remote_sender: to_tx.clone(),
                                }),
                                Box::new(VnetConnectionReceiver {
                                    remote_peer_id: from_socket_peer,
                                    conn_id,
                                    remote_addr: to_socket_addr,
                                    recv: from_rx,
                                }),
                            )))
                            .unwrap();
                        to_socket_sender
                            .send_blocking(VnetListenerEvent::Incoming((
                                Arc::new(VnetConnectionSender {
                                    remote_peer_id: from_socket_peer,
                                    conn_id,
                                    remote_addr: from_socket_addr.clone(),
                                    sender: to_tx,
                                    remote_sender: from_tx,
                                }),
                                Box::new(VnetConnectionReceiver {
                                    remote_peer_id: from_socket_peer,
                                    conn_id,
                                    remote_addr: from_socket_addr,
                                    recv: to_rx,
                                }),
                            )))
                            .unwrap();
                    }
                });
                from_socket
                    .sender
                    .send_blocking(VnetListenerEvent::OutgoingRequest(
                        to_peer,
                        conn_id,
                        outgoing_acceptor,
                    ));
                to_socket
                    .sender
                    .send_blocking(VnetListenerEvent::IncomingRequest(
                        from_socket_peer,
                        conn_id,
                        incoming_acceptor,
                    ));
            } else {
                from_socket
                    .sender
                    .send_blocking(VnetListenerEvent::OutgoingErr(
                        conn_id,
                        to_peer,
                        OutgoingConnectionError::AuthenticationError,
                    ));
            }
        } else {
            from_socket
                .sender
                .send_blocking(VnetListenerEvent::OutgoingErr(
                    conn_id,
                    to_peer,
                    OutgoingConnectionError::DestinationNotFound,
                ));
        }

        Some(conn_id)
    }
}

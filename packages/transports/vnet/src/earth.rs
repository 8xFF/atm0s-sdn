use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use kanal::{Sender, unbounded_async};
use parking_lot::RwLock;
use bluesea_identity::{PeerAddr, PeerId};
use network::transport::OutgoingConnectionError;
use crate::connection::{VnetConnection, VnetConnectionReceiver, VnetConnectionSender};
use crate::listener::{VnetListener, VnetListenerEvent};

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
            ports: Default::default()
        }
    }
}

impl<MSG> VnetEarth<MSG> {
    pub fn create_listener(&self, port: u64, peer: PeerId, addr: PeerAddr) -> VnetListener<MSG> {
        let (tx, rx) = unbounded_async();
        self.ports.write().insert(port, Socket {
            peer,
            addr,
            sender: tx.to_sync()
        });
        VnetListener {
            rx
        }
    }

    pub fn create_outgoing(&self, from_port: u64, to_peer: PeerId, to_port: u64) -> Option<u32> {
        assert_ne!(from_port, to_port);
        let ports = self.ports.read();
        let from_socket = ports.get(&from_port)?;
        let conn_id = self.conn_id_seed.fetch_add(1, Ordering::Relaxed);
        if let Some(to_socket) = ports.get(&to_port) {
            if to_socket.peer == to_peer {
                let (from_tx, from_rx) = unbounded_async();
                let (to_tx, to_rx) = unbounded_async();

                from_socket.sender.send(VnetListenerEvent::Outgoing((
                    Arc::new(VnetConnectionSender {
                        remote_peer_id: to_socket.peer,
                        conn_id,
                        remote_addr: to_socket.addr.clone(),
                        sender: from_tx.clone_sync(),
                        remote_sender: to_tx.clone_sync(),
                    }),
                    Box::new(VnetConnectionReceiver {
                        remote_peer_id: from_socket.peer,
                        conn_id,
                        remote_addr: from_socket.addr.clone(),
                        recv: from_rx
                    })
                ))).unwrap();
                to_socket.sender.send(VnetListenerEvent::Incoming((
                    Arc::new(VnetConnectionSender {
                        remote_peer_id: from_socket.peer,
                        conn_id,
                        remote_addr: from_socket.addr.clone(),
                        sender: to_tx.to_sync(),
                        remote_sender: from_tx.to_sync(),
                    }),
                    Box::new(VnetConnectionReceiver {
                        remote_peer_id: from_socket.peer,
                        conn_id,
                        remote_addr: from_socket.addr.clone(),
                        recv: to_rx
                    })
                ))).unwrap();
            } else {
                from_socket.sender.send(VnetListenerEvent::OutgoingErr(conn_id, to_peer, OutgoingConnectionError::AuthenticationError));
            }
        } else {
            from_socket.sender.send(VnetListenerEvent::OutgoingErr(conn_id, to_peer, OutgoingConnectionError::DestinationNotFound));
        }

        Some(conn_id)
    }
}
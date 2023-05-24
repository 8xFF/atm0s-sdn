use crate::transport::{ConnectionMsg, ConnectionSender};
use async_std::channel::{unbounded, Receiver, Sender};
use bluesea_identity::PeerId;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) enum CrossHandlerEvent<HE> {
    FromBehavior(HE),
    FromHandler(PeerId, u32, HE),
}

#[derive(Debug)]
pub enum CrossHandlerRoute {
    PeerFirst(PeerId),
    Conn(u32),
}

pub(crate) struct CrossHandlerGate<HE, MSG> {
    peers: HashMap<
        PeerId,
        HashMap<
            u32,
            (
                Sender<(u8, CrossHandlerEvent<HE>)>,
                Arc<dyn ConnectionSender<MSG>>,
            ),
        >,
    >,
    conns: HashMap<
        u32,
        (
            Sender<(u8, CrossHandlerEvent<HE>)>,
            Arc<dyn ConnectionSender<MSG>>,
        ),
    >,
}

impl<HE, MSG> Default for CrossHandlerGate<HE, MSG> {
    fn default() -> Self {
        Self {
            peers: Default::default(),
            conns: Default::default(),
        }
    }
}

impl<HE, MSG> CrossHandlerGate<HE, MSG>
where
    HE: Send + Sync + 'static,
    MSG: Send + Sync + 'static,
{
    pub(crate) fn add_conn(
        &mut self,
        net_sender: Arc<dyn ConnectionSender<MSG>>,
    ) -> Option<Receiver<(u8, CrossHandlerEvent<HE>)>> {
        if !self.conns.contains_key(&net_sender.connection_id()) {
            log::info!(
                "[CrossHandlerGate] add_con {} {}",
                net_sender.remote_peer_id(),
                net_sender.connection_id()
            );
            let (tx, rx) = unbounded();
            let entry = self
                .peers
                .entry(net_sender.remote_peer_id())
                .or_insert_with(|| HashMap::new());
            self.conns
                .insert(net_sender.connection_id(), (tx.clone(), net_sender.clone()));
            entry.insert(net_sender.connection_id(), (tx.clone(), net_sender.clone()));
            Some(rx)
        } else {
            log::warn!(
                "[CrossHandlerGate] add_conn duplicate {}",
                net_sender.connection_id()
            );
            None
        }
    }

    pub(crate) fn remove_conn(&mut self, peer: PeerId, conn: u32) -> Option<()> {
        if self.conns.contains_key(&conn) {
            log::info!("[CrossHandlerGate] remove_con {} {}", peer, conn);
            self.conns.remove(&conn);
            let entry = self.peers.entry(peer).or_insert_with(|| HashMap::new());
            entry.remove(&conn);
            if entry.is_empty() {
                self.peers.remove(&peer);
            }
            Some(())
        } else {
            log::warn!("[CrossHandlerGate] remove_conn not found {}", conn);
            None
        }
    }

    pub(crate) fn close_conn(&self, conn: u32) {
        if let Some((s, c_s)) = self.conns.get(&conn) {
            log::info!(
                "[CrossHandlerGate] close_con {} {}",
                c_s.remote_peer_id(),
                conn
            );
            c_s.close();
        } else {
            log::warn!("[CrossHandlerGate] close_conn not found {}", conn);
        }
    }

    pub(crate) fn close_peer(&self, peer: PeerId) {
        if let Some(conns) = self.peers.get(&peer) {
            for (_conn_id, (s, c_s)) in conns {
                log::info!(
                    "[CrossHandlerGate] close_peer {} {}",
                    peer,
                    c_s.connection_id()
                );
                c_s.close();
            }
        }
    }

    pub(crate) fn send_to_handler(
        &self,
        service_id: u8,
        route: CrossHandlerRoute,
        event: CrossHandlerEvent<HE>,
    ) -> Option<()> {
        log::debug!(
            "[CrossHandlerGate] send_to_handler service: {} route: {:?}",
            service_id,
            route
        );
        match route {
            CrossHandlerRoute::PeerFirst(peer_id) => {
                if let Some(peer) = self.peers.get(&peer_id) {
                    if let Some((s, c_s)) = peer.values().next() {
                        if let Err(e) = s.send_blocking((service_id, event)) {
                            log::error!("[CrossHandlerGate] send to handle error {:?}", e);
                        } else {
                            return Some(());
                        }
                    } else {
                        log::warn!(
                            "[CrossHandlerGate] send_to_handler conn not found for peer {}",
                            peer_id
                        );
                    }
                } else {
                    log::warn!(
                        "[CrossHandlerGate] send_to_handler peer not found {}",
                        peer_id
                    );
                }
            }
            CrossHandlerRoute::Conn(conn) => {
                if let Some((s, c_s)) = self.conns.get(&conn) {
                    if let Err(e) = s.send_blocking((service_id, event)) {
                        log::error!("[CrossHandlerGate] send to handle error {:?}", e);
                    } else {
                        return Some(());
                    }
                } else {
                    log::warn!("[CrossHandlerGate] send_to_handler conn not found {}", conn);
                }
            }
        };
        None
    }

    pub(crate) fn send_to_net(
        &self,
        service_id: u8,
        route: CrossHandlerRoute,
        msg: ConnectionMsg<MSG>,
    ) -> Option<()> {
        log::debug!(
            "[CrossHandlerGate] send_to_net service: {} route: {:?}",
            service_id,
            route
        );
        match route {
            CrossHandlerRoute::PeerFirst(peer_id) => {
                if let Some(peer) = self.peers.get(&peer_id) {
                    if let Some((s, c_s)) = peer.values().next() {
                        c_s.send(service_id, msg);
                        return Some(());
                    } else {
                        log::warn!(
                            "[CrossHandlerGate] send_to_handler conn not found for peer {}",
                            peer_id
                        );
                    }
                } else {
                    log::warn!("[CrossHandlerGate] send_to_net peer not found {}", peer_id);
                }
            }
            CrossHandlerRoute::Conn(conn) => {
                if let Some((s, c_s)) = self.conns.get(&conn) {
                    c_s.send(service_id, msg);
                    return Some(());
                } else {
                    log::warn!("[CrossHandlerGate] send_to_net conn not found {}", conn);
                }
            }
        };
        None
    }
}

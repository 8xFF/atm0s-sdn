use crate::transport::{ConnectionMsg, ConnectionSender};
use async_std::channel::{unbounded, Receiver, Sender};
use bluesea_identity::{ConnId, NodeId};
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) enum CrossHandlerEvent<HE> {
    FromBehavior(HE),
    FromHandler(NodeId, ConnId, HE),
}

#[derive(Debug)]
pub enum CrossHandlerRoute {
    NodeFirst(NodeId),
    Conn(ConnId),
}

pub(crate) struct CrossHandlerGate<HE, MSG> {
    nodes: HashMap<
        NodeId,
        HashMap<
            ConnId,
            (
                Sender<(u8, CrossHandlerEvent<HE>)>,
                Arc<dyn ConnectionSender<MSG>>,
            ),
        >,
    >,
    conns: HashMap<
        ConnId,
        (
            Sender<(u8, CrossHandlerEvent<HE>)>,
            Arc<dyn ConnectionSender<MSG>>,
        ),
    >,
}

impl<HE, MSG> Default for CrossHandlerGate<HE, MSG> {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
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
        if !self.conns.contains_key(&net_sender.conn_id()) {
            log::info!(
                "[CrossHandlerGate] add_con {} {}",
                net_sender.remote_node_id(),
                net_sender.conn_id()
            );
            let (tx, rx) = unbounded();
            let entry = self
                .nodes
                .entry(net_sender.remote_node_id())
                .or_insert_with(|| HashMap::new());
            self.conns
                .insert(net_sender.conn_id(), (tx.clone(), net_sender.clone()));
            entry.insert(net_sender.conn_id(), (tx.clone(), net_sender.clone()));
            Some(rx)
        } else {
            log::warn!(
                "[CrossHandlerGate] add_conn duplicate {}",
                net_sender.conn_id()
            );
            None
        }
    }

    pub(crate) fn remove_conn(&mut self, node: NodeId, conn: ConnId) -> Option<()> {
        if self.conns.contains_key(&conn) {
            log::info!("[CrossHandlerGate] remove_con {} {}", node, conn);
            self.conns.remove(&conn);
            let entry = self.nodes.entry(node).or_insert_with(|| HashMap::new());
            entry.remove(&conn);
            if entry.is_empty() {
                self.nodes.remove(&node);
            }
            Some(())
        } else {
            log::warn!("[CrossHandlerGate] remove_conn not found {}", conn);
            None
        }
    }

    pub(crate) fn close_conn(&self, conn: ConnId) {
        if let Some((s, c_s)) = self.conns.get(&conn) {
            log::info!(
                "[CrossHandlerGate] close_con {} {}",
                c_s.remote_node_id(),
                conn
            );
            c_s.close();
        } else {
            log::warn!("[CrossHandlerGate] close_conn not found {}", conn);
        }
    }

    pub(crate) fn close_node(&self, node: NodeId) {
        if let Some(conns) = self.nodes.get(&node) {
            for (_conn_id, (s, c_s)) in conns {
                log::info!("[CrossHandlerGate] close_node {} {}", node, c_s.conn_id());
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
            CrossHandlerRoute::NodeFirst(node_id) => {
                if let Some(node) = self.nodes.get(&node_id) {
                    if let Some((s, c_s)) = node.values().next() {
                        if let Err(e) = s.send_blocking((service_id, event)) {
                            log::error!("[CrossHandlerGate] send to handle error {:?}", e);
                        } else {
                            return Some(());
                        }
                    } else {
                        log::warn!(
                            "[CrossHandlerGate] send_to_handler conn not found for node {}",
                            node_id
                        );
                    }
                } else {
                    log::warn!(
                        "[CrossHandlerGate] send_to_handler node not found {}",
                        node_id
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
            CrossHandlerRoute::NodeFirst(node_id) => {
                if let Some(node) = self.nodes.get(&node_id) {
                    if let Some((s, c_s)) = node.values().next() {
                        c_s.send(service_id, msg);
                        return Some(());
                    } else {
                        log::warn!(
                            "[CrossHandlerGate] send_to_handler conn not found for node {}",
                            node_id
                        );
                    }
                } else {
                    log::warn!("[CrossHandlerGate] send_to_net node not found {}", node_id);
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

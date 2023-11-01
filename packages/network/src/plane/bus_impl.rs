use crate::plane::NetworkPlaneInternalEvent;
use crate::transport::ConnectionSender;
use crate::{msg::TransportMsg, plane::bus::HandlerRoute};
use async_std::channel::{unbounded, Receiver, Sender};
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::{RouteAction, RouteRule, RouterTable};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use utils::error_handle::ErrorUtils;

use super::bus::{HandleEvent, PlaneBus};

pub(crate) struct PlaneBusImpl<BE, HE> {
    node_id: NodeId,
    plane_tx: Sender<NetworkPlaneInternalEvent<BE>>,
    nodes: RwLock<HashMap<NodeId, HashMap<ConnId, (Sender<(u8, HandleEvent<HE>)>, Arc<dyn ConnectionSender>)>>>,
    conns: RwLock<HashMap<ConnId, (Sender<(u8, HandleEvent<HE>)>, Arc<dyn ConnectionSender>)>>,
    router: Arc<dyn RouterTable>,
}

impl<HE, BE> PlaneBusImpl<BE, HE>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    pub fn new(node_id: NodeId, router: Arc<dyn RouterTable>, plane_tx: Sender<NetworkPlaneInternalEvent<BE>>) -> Self {
        Self {
            node_id,
            plane_tx,
            nodes: Default::default(),
            conns: Default::default(),
            router,
        }
    }

    pub(crate) fn add_conn(&self, net_sender: Arc<dyn ConnectionSender>) -> Option<Receiver<(u8, HandleEvent<HE>)>> {
        let mut conns = self.conns.write();
        let mut nodes = self.nodes.write();
        if let std::collections::hash_map::Entry::Vacant(e) = conns.entry(net_sender.conn_id()) {
            log::info!("[PlaneBusImpl {}] add_con {} {}", self.node_id, net_sender.remote_node_id(), net_sender.conn_id());
            let (tx, rx) = unbounded();
            let entry = nodes.entry(net_sender.remote_node_id()).or_insert_with(HashMap::new);
            e.insert((tx.clone(), net_sender.clone()));
            entry.insert(net_sender.conn_id(), (tx.clone(), net_sender.clone()));
            Some(rx)
        } else {
            log::warn!("[PlaneBusImpl {}] add_conn duplicate {}", self.node_id, net_sender.conn_id());
            None
        }
    }

    pub(crate) fn remove_conn(&self, node: NodeId, conn: ConnId) -> Option<()> {
        let mut conns = self.conns.write();
        let mut nodes = self.nodes.write();
        if conns.contains_key(&conn) {
            log::info!("[PlaneBusImpl {}] remove_con {} {}", self.node_id, node, conn);
            conns.remove(&conn);
            let entry = nodes.entry(node).or_insert_with(HashMap::new);
            entry.remove(&conn);
            if entry.is_empty() {
                nodes.remove(&node);
            }
            Some(())
        } else {
            log::warn!("[PlaneBusImpl {}] remove_conn not found {}", self.node_id, conn);
            None
        }
    }

    pub(crate) fn close_conn(&self, conn: ConnId) {
        let conns = self.conns.read();
        if let Some((_s, c_s)) = conns.get(&conn) {
            log::info!("[PlaneBusImpl {}] close_con {} {}", self.node_id, c_s.remote_node_id(), conn);
            c_s.close();
        } else {
            log::warn!("[PlaneBusImpl {}] close_conn not found {}", self.node_id, conn);
        }
    }

    pub(crate) fn close_node(&self, node: NodeId) {
        let nodes = self.nodes.read();
        if let Some(conns) = nodes.get(&node) {
            for (_s, c_s) in conns.values() {
                log::info!("[PlaneBusImpl {}] close_node {} {}", self.node_id, node, c_s.conn_id());
                c_s.close();
            }
        }
    }
}

impl<BE, HE> PlaneBus<BE, HE> for PlaneBusImpl<BE, HE>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    fn awake_behaviour(&self, service_id: u8) -> Option<()> {
        if let Err(e) = self.plane_tx.send_blocking(NetworkPlaneInternalEvent::AwakeBehaviour { service_id }) {
            log::error!("[PlaneBusImpl {}] send to behaviour error {:?}", self.node_id, e);
            None
        } else {
            Some(())
        }
    }

    fn awake_handler(&self, service_id: u8, conn: ConnId) -> Option<()> {
        self.to_handler(service_id, HandlerRoute::Conn(conn), HandleEvent::Awake)
    }

    fn to_behaviour_from_handler(&self, service_id: u8, node_id: NodeId, conn_id: ConnId, event: BE) -> Option<()> {
        if let Err(e) = self.plane_tx.send_blocking(NetworkPlaneInternalEvent::ToBehaviourFromHandler { service_id, node_id, conn_id, event }) {
            log::error!("[PlaneBusImpl {}] send to behaviour error {:?}", self.node_id, e);
            None
        } else {
            Some(())
        }
    }

    fn to_handler(&self, service_id: u8, route: HandlerRoute, event: HandleEvent<HE>) -> Option<()> {
        log::debug!("[PlaneBusImpl {}] send_to_handler service: {} route: {:?}", self.node_id, service_id, route);
        match route {
            HandlerRoute::NodeFirst(node_id) => {
                if let Some(node) = self.nodes.read().get(&node_id) {
                    if let Some((s, _c_s)) = node.values().next() {
                        if let Err(e) = s.send_blocking((service_id, event)) {
                            log::error!("[PlaneBusImpl {}] send to handle error {:?}", self.node_id, e);
                        } else {
                            return Some(());
                        }
                    } else {
                        log::warn!("[PlaneBusImpl {}] send_to_handler conn not found for node {}", self.node_id, node_id);
                    }
                } else {
                    log::warn!("[PlaneBusImpl {}] send_to_handler node not found {}", self.node_id, node_id);
                }
            }
            HandlerRoute::Conn(conn) => {
                if let Some((s, _c_s)) = self.conns.read().get(&conn) {
                    if let Err(e) = s.send_blocking((service_id, event)) {
                        log::error!("[PlaneBusImpl {}] send to handle error {:?}", self.node_id, e);
                    } else {
                        return Some(());
                    }
                } else {
                    log::warn!("[PlaneBusImpl {}] send_to_handler conn not found {}", self.node_id, conn);
                }
            }
        };
        None
    }

    fn to_net(&self, msg: TransportMsg) -> Option<()> {
        log::debug!("[PlaneBusImpl {}] send_to_net service: {} route: {:?}", self.node_id, msg.header.service_id, msg.header.route);
        match self.router.action_for_incomming(&msg.header.route, msg.header.service_id) {
            RouteAction::Reject => {
                log::warn!("[PlaneBusImpl {}] send_to_net reject {} {:?}", self.node_id, msg.header.service_id, msg.header.route);
                None
            }
            RouteAction::Local => {
                log::debug!("[PlaneBusImpl {}] send_to_net service: {} route: {:?} => local", self.node_id, msg.header.service_id, msg.header.route);
                // TODO: may be have other way to send to local without serializing and deserializing
                self.plane_tx
                    .send_blocking(NetworkPlaneInternalEvent::ToBehaviourLocalMsg {
                        service_id: msg.header.service_id,
                        msg: msg,
                    })
                    .print_error("Should send to behaviour transport msg");
                Some(())
            }
            RouteAction::Next(conn, _node) => {
                if let Some((_s, c_s)) = self.conns.read().get(&conn) {
                    log::debug!(
                        "[PlaneBusImpl {}] send_to_net service: {} route: {:?} => next conn ({})",
                        self.node_id,
                        msg.header.service_id,
                        msg.header.route,
                        conn
                    );
                    c_s.send(msg);
                    Some(())
                } else {
                    log::warn!("[PlaneBusImpl {}] send_to_net conn not found {}", self.node_id, conn);
                    None
                }
            }
        }
    }

    fn to_net_node(&self, node: NodeId, msg: TransportMsg) -> Option<()> {
        log::debug!("[PlaneBusImpl {}] send_to_net_node service: {} route: ToNode({})", self.node_id, msg.header.service_id, node);
        match self.router.action_for_incomming(&RouteRule::ToNode(node), msg.header.service_id) {
            RouteAction::Reject => {
                log::warn!("[PlaneBusImpl {}] send_to_net reject {} ToNode({})", self.node_id, msg.header.service_id, node);
                None
            }
            RouteAction::Local => {
                // TODO: may be have other way to send to local without serializing and deserializing
                self.plane_tx
                    .send_blocking(NetworkPlaneInternalEvent::ToBehaviourLocalMsg {
                        service_id: msg.header.service_id,
                        msg: msg,
                    })
                    .print_error("Should send to behaviour transport msg");
                Some(())
            }
            RouteAction::Next(conn, _node) => {
                if let Some((_s, c_s)) = self.conns.read().get(&conn) {
                    c_s.send(msg);
                    Some(())
                } else {
                    log::warn!("[PlaneBusImpl {}] send_to_net_node conn not found {}", self.node_id, conn);
                    None
                }
            }
        }
    }

    fn to_net_conn(&self, conn_id: ConnId, msg: TransportMsg) -> Option<()> {
        log::debug!("[PlaneBusImpl {}] send_to_net_direct service: {} conn: {}", self.node_id, msg.header.service_id, conn_id);
        let conns = self.conns.read();
        let (_, sender) = conns.get(&conn_id)?;
        sender.send(msg);
        Some(())
    }
}

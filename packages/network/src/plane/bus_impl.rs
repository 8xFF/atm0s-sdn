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
    pub fn new(router: Arc<dyn RouterTable>, plane_tx: Sender<NetworkPlaneInternalEvent<BE>>) -> Self {
        Self {
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
            log::info!("[CrossHandlerGate] add_con {} {}", net_sender.remote_node_id(), net_sender.conn_id());
            let (tx, rx) = unbounded();
            let entry = nodes.entry(net_sender.remote_node_id()).or_insert_with(HashMap::new);
            e.insert((tx.clone(), net_sender.clone()));
            entry.insert(net_sender.conn_id(), (tx.clone(), net_sender.clone()));
            Some(rx)
        } else {
            log::warn!("[CrossHandlerGate] add_conn duplicate {}", net_sender.conn_id());
            None
        }
    }

    pub(crate) fn remove_conn(&self, node: NodeId, conn: ConnId) -> Option<()> {
        let mut conns = self.conns.write();
        let mut nodes = self.nodes.write();
        if conns.contains_key(&conn) {
            log::info!("[CrossHandlerGate] remove_con {} {}", node, conn);
            conns.remove(&conn);
            let entry = nodes.entry(node).or_insert_with(HashMap::new);
            entry.remove(&conn);
            if entry.is_empty() {
                nodes.remove(&node);
            }
            Some(())
        } else {
            log::warn!("[CrossHandlerGate] remove_conn not found {}", conn);
            None
        }
    }

    pub(crate) fn close_conn(&self, conn: ConnId) {
        let conns = self.conns.read();
        if let Some((_s, c_s)) = conns.get(&conn) {
            log::info!("[CrossHandlerGate] close_con {} {}", c_s.remote_node_id(), conn);
            c_s.close();
        } else {
            log::warn!("[CrossHandlerGate] close_conn not found {}", conn);
        }
    }

    pub(crate) fn close_node(&self, node: NodeId) {
        let nodes = self.nodes.read();
        if let Some(conns) = nodes.get(&node) {
            for (_s, c_s) in conns.values() {
                log::info!("[CrossHandlerGate] close_node {} {}", node, c_s.conn_id());
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
    fn to_behaviour(&self, service_id: u8, event: BE) -> Option<()> {
        if let Err(e) = self.plane_tx.send_blocking(NetworkPlaneInternalEvent::ToBehaviourLocalEvent { service_id, event }) {
            log::error!("[CrossHandlerGate] send to behaviour error {:?}", e);
            None
        } else {
            Some(())
        }
    }

    fn to_handler(&self, service_id: u8, route: HandlerRoute, event: HandleEvent<HE>) -> Option<()> {
        log::debug!("[CrossHandlerGate] send_to_handler service: {} route: {:?}", service_id, route);
        match route {
            HandlerRoute::NodeFirst(node_id) => {
                if let Some(node) = self.nodes.read().get(&node_id) {
                    if let Some((s, _c_s)) = node.values().next() {
                        if let Err(e) = s.send_blocking((service_id, event)) {
                            log::error!("[CrossHandlerGate] send to handle error {:?}", e);
                        } else {
                            return Some(());
                        }
                    } else {
                        log::warn!("[CrossHandlerGate] send_to_handler conn not found for node {}", node_id);
                    }
                } else {
                    log::warn!("[CrossHandlerGate] send_to_handler node not found {}", node_id);
                }
            }
            HandlerRoute::Conn(conn) => {
                if let Some((s, _c_s)) = self.conns.read().get(&conn) {
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

    fn to_net(&self, msg: TransportMsg) -> Option<()> {
        log::debug!("[CrossHandlerGate] send_to_net service: {} route: {:?}", msg.header.service_id, msg.header.route);
        match self.router.action_for_incomming(&msg.header.route, msg.header.service_id) {
            RouteAction::Reject => {
                log::warn!("[CrossHandlerGate] send_to_net reject {} {:?}", msg.header.service_id, msg.header.route);
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
                    log::warn!("[CrossHandlerGate] send_to_net conn not found {}", conn);
                    None
                }
            }
        }
    }

    fn to_net_node(&self, node: NodeId, msg: TransportMsg) -> Option<()> {
        log::debug!("[CrossHandlerGate] send_to_net_node service: {} route: ToNode({})", msg.header.service_id, node);
        match self.router.action_for_incomming(&RouteRule::ToNode(node), msg.header.service_id) {
            RouteAction::Reject => {
                log::warn!("[CrossHandlerGate] send_to_net reject {} ToNode({})", msg.header.service_id, node);
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
                    log::warn!("[CrossHandlerGate] send_to_net_node conn not found {}", conn);
                    None
                }
            }
        }
    }

    fn to_net_conn(&self, conn_id: ConnId, msg: TransportMsg) -> Option<()> {
        log::debug!("[CrossHandlerGate] send_to_net_direct service: {} conn: {}", msg.header.service_id, conn_id);
        let conns = self.conns.read();
        let (_, sender) = conns.get(&conn_id)?;
        sender.send(msg);
        Some(())
    }
}

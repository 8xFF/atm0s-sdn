use crate::msg::TransportMsg;
use crate::plane::NetworkPlaneInternalEvent;
use crate::transport::{ConnectionSender, OutgoingConnectionError, TransportConnectingOutgoing, TransportConnector};
use async_std::channel::Sender;
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use parking_lot::RwLock;
use std::sync::Arc;

use super::{CrossHandlerEvent, CrossHandlerGate, CrossHandlerRoute};

pub struct BehaviorAgent<BE, HE> {
    pub(crate) service_id: u8,
    pub(crate) local_node_id: NodeId,
    pub(crate) connector: Arc<dyn TransportConnector>,
    pub(crate) cross_gate: Arc<RwLock<dyn CrossHandlerGate<BE, HE>>>,
}

impl<BE, HE> Clone for BehaviorAgent<BE, HE> {
    fn clone(&self) -> Self {
        Self {
            service_id: self.service_id,
            local_node_id: self.local_node_id,
            connector: self.connector.clone(),
            cross_gate: self.cross_gate.clone(),
        }
    }
}

impl<BE, HE> BehaviorAgent<BE, HE>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    pub(crate) fn new(service_id: u8, local_node_id: NodeId, connector: Arc<dyn TransportConnector>, cross_gate: Arc<RwLock<dyn CrossHandlerGate<BE, HE>>>) -> Self {
        Self {
            service_id,
            connector,
            local_node_id,
            cross_gate,
        }
    }

    pub fn local_node_id(&self) -> NodeId {
        self.local_node_id
    }

    pub fn connect_to(&self, node_id: NodeId, dest: NodeAddr) -> Result<TransportConnectingOutgoing, OutgoingConnectionError> {
        self.connector.connect_to(node_id, dest)
    }

    pub fn send_to_behaviour(&self, event: BE) {
        self.cross_gate.read().send_to_behaviour(self.service_id, event);
    }

    pub fn send_to_handler(&self, route: CrossHandlerRoute, event: HE) {
        self.cross_gate.read().send_to_handler(self.service_id, route, CrossHandlerEvent::FromBehavior(event));
    }

    pub fn send_to_net(&self, msg: TransportMsg) {
        self.cross_gate.read().send_to_net(msg);
    }

    pub fn send_to_net_node(&self, node: NodeId, msg: TransportMsg) {
        self.cross_gate.read().send_to_net_node(node, msg);
    }

    pub fn send_to_net_direct(&self, conn: ConnId, msg: TransportMsg) {
        self.cross_gate.read().send_to_net_direct(conn, msg);
    }

    pub fn close_conn(&self, conn: ConnId) {
        self.cross_gate.read().close_conn(conn);
    }

    pub fn close_node(&self, node: NodeId) {
        self.cross_gate.read().close_node(node);
    }
}

pub struct ConnectionAgent<BE, HE> {
    service_id: u8,
    local_node_id: NodeId,
    remote_node_id: NodeId,
    conn_id: ConnId,
    sender: Arc<dyn ConnectionSender>,
    internal_tx: Sender<NetworkPlaneInternalEvent<BE>>,
    cross_gate: Arc<RwLock<dyn CrossHandlerGate<BE, HE>>>,
}

impl<BE, HE> Clone for ConnectionAgent<BE, HE> {
    fn clone(&self) -> Self {
        Self {
            service_id: self.service_id,
            local_node_id: self.local_node_id,
            remote_node_id: self.remote_node_id,
            conn_id: self.conn_id,
            sender: self.sender.clone(),
            internal_tx: self.internal_tx.clone(),
            cross_gate: self.cross_gate.clone(),
        }
    }
}

impl<BE, HE> ConnectionAgent<BE, HE>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    pub(crate) fn new(
        service_id: u8,
        local_node_id: NodeId,
        remote_node_id: NodeId,
        conn_id: ConnId,
        sender: Arc<dyn ConnectionSender>,
        internal_tx: Sender<NetworkPlaneInternalEvent<BE>>,
        cross_gate: Arc<RwLock<dyn CrossHandlerGate<BE, HE>>>,
    ) -> Self {
        Self {
            service_id,
            local_node_id,
            remote_node_id,
            conn_id,
            sender,
            internal_tx,
            cross_gate,
        }
    }

    pub fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    pub fn local_node_id(&self) -> NodeId {
        self.local_node_id
    }

    pub fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    pub fn send_behavior(&self, event: BE) {
        match self.internal_tx.send_blocking(NetworkPlaneInternalEvent::ToBehaviourFromHandler {
            service_id: self.service_id,
            node_id: self.remote_node_id,
            conn_id: self.conn_id,
            event,
        }) {
            Ok(_) => {}
            Err(err) => {
                log::error!("send event to Behavior error {:?}", err);
            }
        }
    }

    pub fn send_net(&self, msg: TransportMsg) {
        self.sender.send(msg);
    }

    pub fn send_to_handler(&self, route: CrossHandlerRoute, event: HE) {
        self.cross_gate
            .read()
            .send_to_handler(self.service_id, route, CrossHandlerEvent::FromHandler(self.remote_node_id, self.conn_id, event));
    }

    pub fn send_to_net(&self, msg: TransportMsg) {
        self.cross_gate.read().send_to_net(msg);
    }

    pub fn send_to_net_direct(&self, conn_id: ConnId, msg: TransportMsg) -> Option<()> {
        self.cross_gate.read().send_to_net_direct(conn_id, msg)
    }

    pub fn close_conn(&self) {
        self.cross_gate.read().close_conn(self.conn_id);
    }
}

use crate::internal::cross_handler_gate::{CrossHandlerEvent, CrossHandlerGate, CrossHandlerRoute};
use crate::plane::NetworkPlaneInternalEvent;
use crate::transport::{ConnectionMsg, ConnectionSender, MsgRoute, OutgoingConnectionError, TransportConnector, TransportPendingOutgoing};
use async_std::channel::Sender;
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct BehaviorAgent<HE, MSG> {
    service_id: u8,
    local_node_id: NodeId,
    connector: Arc<dyn TransportConnector>,
    cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
}

impl<HE, MSG> BehaviorAgent<HE, MSG>
where
    HE: Send + Sync + 'static,
    MSG: Send + Sync + 'static,
{
    pub(crate) fn new(
        service_id: u8,
        local_node_id: NodeId,
        connector: Arc<dyn TransportConnector>,
        cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
    ) -> Self {
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

    pub fn connect_to(
        &self,
        node_id: NodeId,
        dest: NodeAddr,
    ) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        self.connector.connect_to(node_id, dest)
    }

    pub fn send_to_handler(&self, route: CrossHandlerRoute, event: HE) {
        self.cross_gate.read().send_to_handler(
            self.service_id,
            route,
            CrossHandlerEvent::FromBehavior(event),
        );
    }

    pub fn send_to_net(&self, route: MsgRoute, ttl: u8, msg: ConnectionMsg<MSG>) {
        self.cross_gate
            .read()
            .send_to_net(route, ttl, self.service_id, msg);
    }

    pub fn close_conn(&self, conn: ConnId) {
        self.cross_gate.read().close_conn(conn);
    }

    pub fn close_node(&self, node: NodeId) {
        self.cross_gate.read().close_node(node);
    }
}

pub struct ConnectionAgent<BE, HE, MSG> {
    service_id: u8,
    local_node_id: NodeId,
    remote_node_id: NodeId,
    conn_id: ConnId,
    sender: Arc<dyn ConnectionSender<MSG>>,
    internal_tx: Sender<NetworkPlaneInternalEvent<BE, MSG>>,
    cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
}

impl<BE, HE, MSG> ConnectionAgent<BE, HE, MSG>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
    MSG: Send + Sync + 'static,
{
    pub(crate) fn new(
        service_id: u8,
        local_node_id: NodeId,
        remote_node_id: NodeId,
        conn_id: ConnId,
        sender: Arc<dyn ConnectionSender<MSG>>,
        internal_tx: Sender<NetworkPlaneInternalEvent<BE, MSG>>,
        cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
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
        match self
            .internal_tx
            .send_blocking(NetworkPlaneInternalEvent::ToBehaviour {
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

    pub fn send_net(&self, msg: ConnectionMsg<MSG>) {
        self.sender.send(MsgRoute::Node(self.remote_node_id), 1, self.service_id, msg);
    }

    pub fn send_to_handler(&self, route: CrossHandlerRoute, event: HE) {
        self.cross_gate.read().send_to_handler(
            self.service_id,
            route,
            CrossHandlerEvent::FromHandler(self.remote_node_id, self.conn_id, event),
        );
    }

    pub fn send_to_net(&self, route: MsgRoute, ttl: u8, msg: ConnectionMsg<MSG>) {
        self.cross_gate
            .read()
            .send_to_net(route, ttl, self.service_id, msg);
    }

    pub fn close_conn(&self) {
        self.cross_gate.read().close_conn(self.conn_id);
    }
}

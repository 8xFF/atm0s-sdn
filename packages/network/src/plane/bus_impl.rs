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
    /// Current NodeId
    node_id: NodeId,
    /// Network plane internal event sender
    plane_tx: Sender<NetworkPlaneInternalEvent<BE>>,
    /// NodeId -> (ConnId -> (Sender, ConnectionSender))
    nodes: RwLock<HashMap<NodeId, HashMap<ConnId, (Sender<(u8, HandleEvent<HE>)>, Arc<dyn ConnectionSender>)>>>,
    /// ConnId -> (Sender, ConnectionSender)
    conns: RwLock<HashMap<ConnId, (Sender<(u8, HandleEvent<HE>)>, Arc<dyn ConnectionSender>)>>,
    /// Router table
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

    /// Add a connection to plane bus.
    /// Return a receiver for the connection.
    pub(crate) fn add_conn(&self, net_sender: Arc<dyn ConnectionSender>) -> Option<Receiver<(u8, HandleEvent<HE>)>> {
        let mut conns = self.conns.write();
        let mut nodes = self.nodes.write();
        if let std::collections::hash_map::Entry::Vacant(conn_entry) = conns.entry(net_sender.conn_id()) {
            log::info!("[PlaneBusImpl {}] add_con {} {}", self.node_id, net_sender.remote_node_id(), net_sender.conn_id());
            let (tx, rx) = unbounded();
            let node_conns = nodes.entry(net_sender.remote_node_id()).or_insert_with(HashMap::new);
            conn_entry.insert((tx.clone(), net_sender.clone()));
            node_conns.insert(net_sender.conn_id(), (tx.clone(), net_sender.clone()));
            Some(rx)
        } else {
            log::warn!("[PlaneBusImpl {}] add_conn duplicate {}", self.node_id, net_sender.conn_id());
            None
        }
    }

    /// Remove a connection from plane bus.
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

    /// Close a connection.
    pub(crate) fn close_conn(&self, conn: ConnId) {
        let conns = self.conns.read();
        if let Some((_s, c_s)) = conns.get(&conn) {
            log::info!("[PlaneBusImpl {}] close_con {} {}", self.node_id, c_s.remote_node_id(), conn);
            c_s.close();
        } else {
            log::warn!("[PlaneBusImpl {}] close_conn not found {}", self.node_id, conn);
        }
    }

    /// Close every connections to node.
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
        match self.router.derive_action(&msg.header.route, msg.header.service_id) {
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
        match self.router.derive_action(&RouteRule::ToNode(node), msg.header.service_id) {
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

#[cfg(test)]
mod tests {
    use crate::{
        msg::TransportMsg,
        plane::{
            bus::{HandleEvent, HandlerRoute, PlaneBus},
            bus_impl::PlaneBusImpl,
            NetworkPlaneInternalEvent,
        },
        transport::{ConnectionSender, MockConnectionSender},
    };
    use async_std::channel::{unbounded, TryRecvError};
    use bluesea_identity::{ConnId, NodeAddr, NodeId};
    use bluesea_router::{MockRouterTable, RouteAction, RouteRule};
    use std::sync::Arc;

    type HE = ();
    type BE = ();

    fn create_mock_connection(conn: ConnId, node: NodeId, addr: NodeAddr) -> MockConnectionSender {
        let mut sender = MockConnectionSender::new();
        sender.expect_conn_id().return_const(conn);
        sender.expect_remote_node_id().return_const(node);
        sender.expect_remote_addr().return_const(addr);
        sender.expect_send().return_const(());

        sender
    }

    #[test]
    fn should_add_remove_close_connection() {
        let local_node_id = 1;
        let (plane_tx, _plane_rx) = unbounded();
        let router = Arc::new(MockRouterTable::new());
        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);
        let mut sender = create_mock_connection(ConnId::from_in(1, 1), 2u32, NodeAddr::empty());
        sender.expect_close().times(1).return_const(());
        let conn = Arc::new(sender);
        let conn_c = conn.clone();
        let rx = bus.add_conn(conn.clone());
        assert!(rx.is_some());

        // duplicate conn
        assert!(bus.add_conn(conn).is_none());

        assert_eq!(bus.conns.read().len(), 1);
        assert_eq!(bus.nodes.read().len(), 1);
        assert_eq!(bus.nodes.read().get(&conn_c.remote_node_id()).unwrap().len(), 1);
        assert_eq!(bus.nodes.read().get(&conn_c.remote_node_id()).unwrap().get(&conn_c.conn_id()).unwrap().1.conn_id(), conn_c.conn_id());
        assert_eq!(bus.conns.read().get(&conn_c.conn_id()).unwrap().1.conn_id(), conn_c.conn_id());

        // Close conn
        bus.close_conn(conn_c.conn_id());

        // Remove Conn
        bus.remove_conn(2u32, ConnId::from_in(1, 1));
        assert_eq!(bus.conns.read().len(), 0);
        assert_eq!(bus.nodes.read().len(), 0);

        // remove non-exist conn
        assert!(bus.remove_conn(2u32, ConnId::from_in(1, 1)).is_none());
    }

    #[test]
    fn should_close_node() {
        let local_node_id = 1;
        let (plane_tx, _plane_rx) = unbounded();
        let router = Arc::new(MockRouterTable::new());
        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);
        let mut data = create_mock_connection(ConnId::from_in(1, 1), 2u32, NodeAddr::empty());
        data.expect_close().times(1).return_const(());

        let conn = Arc::new(data);
        let rx = bus.add_conn(conn.clone());
        assert!(rx.is_some());

        // duplicate conn
        assert!(bus.add_conn(conn).is_none());

        assert_eq!(bus.conns.read().len(), 1);
        assert_eq!(bus.nodes.read().len(), 1);

        // Close node
        bus.close_node(2u32);
    }

    #[async_std::test]
    async fn should_send_event_to_handler() {
        let local_node_id = 1;
        let (plane_tx, _plane_rx) = unbounded();
        let router = Arc::new(MockRouterTable::new());
        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);
        let data = create_mock_connection(ConnId::from_in(1, 1), 2u32, NodeAddr::empty());

        let conn = Arc::new(data);
        let rx = bus.add_conn(conn.clone()).expect("Should have rx");

        // duplicate conn
        assert!(bus.add_conn(conn).is_none());
        assert_eq!(bus.conns.read().len(), 1);
        assert_eq!(bus.nodes.read().len(), 1);
        // Send event to handler through conn
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        assert!(bus.to_handler(1, HandlerRoute::Conn(ConnId::from_in(1, 1)), HandleEvent::Awake).is_some());
        assert_eq!(rx.try_recv(), Ok((1, HandleEvent::Awake)));

        // Send event to handler through node
        assert!(bus.to_handler(1, HandlerRoute::NodeFirst(2u32), HandleEvent::Awake).is_some());
        assert_eq!(rx.try_recv(), Ok((1, HandleEvent::Awake)));
    }

    #[test]
    fn should_correctly_reject_to_network() {
        let local_node_id = 1;
        let (plane_tx, _plane_rx) = unbounded();
        let mut mock_router = MockRouterTable::new();
        mock_router.expect_derive_action().returning(|_, _| RouteAction::Reject);
        let router = Arc::new(mock_router);

        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);

        assert!(bus.to_net(TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8])).is_none());
    }

    #[async_std::test]
    async fn to_net_should_process_local() {
        let local_node_id = 1;
        let (plane_tx, plane_rx) = unbounded();
        let mut mock_router = MockRouterTable::new();
        mock_router.expect_derive_action().returning(|_, _| RouteAction::Local);
        let router = Arc::new(mock_router);

        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);

        assert_eq!(plane_rx.try_recv(), Err(TryRecvError::Empty));

        assert!(bus.to_net(TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8])).is_some());

        assert_eq!(
            plane_rx.try_recv(),
            Ok(NetworkPlaneInternalEvent::ToBehaviourLocalMsg {
                service_id: 1,
                msg: TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8]),
            })
        );
    }

    #[async_std::test]
    async fn to_net_should_forward() {
        let local_node_id = 1;
        let (plane_tx, plane_rx) = unbounded();
        let mut mock_router = MockRouterTable::new();
        mock_router.expect_derive_action().returning(|_, _| RouteAction::Next(ConnId::from_in(1, 1), 2u32));
        let router = Arc::new(mock_router);

        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);

        let sender = create_mock_connection(ConnId::from_in(1, 1), 2u32, NodeAddr::empty());

        let conn = Arc::new(sender);
        let _rx = bus.add_conn(conn).expect("Should have rx");
        assert_eq!(plane_rx.try_recv(), Err(TryRecvError::Empty));

        assert!(bus.to_net(TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8])).is_some());
    }

    #[async_std::test]
    async fn to_net_node_should_process_local() {
        let local_node_id = 1;
        let (plane_tx, plane_rx) = unbounded();
        let mut mock_router = MockRouterTable::new();
        mock_router.expect_derive_action().returning(|_, _| RouteAction::Local);
        let router = Arc::new(mock_router);

        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);

        assert_eq!(plane_rx.try_recv(), Err(TryRecvError::Empty));

        assert!(bus.to_net_node(2u32, TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8])).is_some());

        assert_eq!(
            plane_rx.try_recv(),
            Ok(NetworkPlaneInternalEvent::ToBehaviourLocalMsg {
                service_id: 1,
                msg: TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8]),
            })
        );
    }

    #[async_std::test]
    async fn to_net_node_should_forward() {
        let local_node_id = 1;
        let (plane_tx, plane_rx) = unbounded();
        let mut mock_router = MockRouterTable::new();
        mock_router.expect_derive_action().returning(|_, _| RouteAction::Next(ConnId::from_in(1, 1), 2u32));
        let router = Arc::new(mock_router);

        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);

        let sender = create_mock_connection(ConnId::from_in(1, 1), 2u32, NodeAddr::empty());

        let conn = Arc::new(sender);
        let _rx = bus.add_conn(conn).expect("Should have rx");
        assert_eq!(plane_rx.try_recv(), Err(TryRecvError::Empty));

        assert!(bus.to_net_node(2u32, TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8])).is_some());
    }

    #[async_std::test]
    async fn to_net_conn_should_forward() {
        let local_node_id = 1;
        let (plane_tx, _plane_rx) = unbounded();
        let mock_router = MockRouterTable::new();
        let router = Arc::new(mock_router);

        let bus = PlaneBusImpl::<BE, HE>::new(local_node_id, router, plane_tx);

        let sender = create_mock_connection(ConnId::from_in(1, 1), 2u32, NodeAddr::empty());

        let conn = Arc::new(sender);
        let _rx = bus.add_conn(conn).expect("Should have rx");
        assert!(bus.to_net_conn(ConnId::from_in(1, 1), TransportMsg::build_unreliable(1, RouteRule::ToService(2), 1, &[1u8])).is_some());
}
}

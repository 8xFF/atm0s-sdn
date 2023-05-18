use crate::behaviour::{ConnectionHandler, NetworkBehavior};
use crate::transport::{
    ConnectionEvent, ConnectionMsg, ConnectionSender, OutgoingConnectionError, Transport,
    TransportConnector, TransportEvent, TransportPendingOutgoing,
};
use async_std::channel::{bounded, unbounded, Receiver, Sender};
use async_std::stream::Interval;
use bluesea_identity::{PeerAddr, PeerId};
use futures::{select, FutureExt, SinkExt, StreamExt};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use utils::Timer;

fn init_vec<T>(size: usize, builder: fn() -> T) -> Vec<T> {
    let mut vec = vec![];
    for _ in 0..size {
        vec.push(builder());
    }
    vec
}

enum CrossHandlerEvent<HE> {
    FromBehavior(HE),
    FromHandler(PeerId, u32, HE),
}

#[derive(Debug)]
pub enum CrossHandlerRoute {
    PeerFirst(PeerId),
    Conn(u32),
}

struct CrossHandlerGate<HE, MSG> {
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
    fn add_conn(
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

    fn remove_conn(&mut self, peer: PeerId, conn: u32) -> Option<()> {
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

    fn close_conn(&self, conn: u32) {
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

    fn close_peer(&self, peer: PeerId) {
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

    fn send_to_handler(
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

    fn send_to_net(
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

pub struct BehaviorAgent<HE, MSG> {
    service_id: u8,
    local_peer_id: PeerId,
    connector: Arc<dyn TransportConnector>,
    cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
}

impl<HE, MSG> BehaviorAgent<HE, MSG>
where
    HE: Send + Sync + 'static,
    MSG: Send + Sync + 'static,
{
    fn new(
        service_id: u8,
        local_peer_id: PeerId,
        connector: Arc<dyn TransportConnector>,
        cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
    ) -> Self {
        Self {
            service_id,
            connector,
            local_peer_id,
            cross_gate,
        }
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub fn connect_to(
        &self,
        peer_id: PeerId,
        dest: PeerAddr,
    ) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        self.connector.connect_to(peer_id, dest)
    }

    pub fn send_to_handler(&self, route: CrossHandlerRoute, event: HE) {
        self.cross_gate.read().send_to_handler(
            self.service_id,
            route,
            CrossHandlerEvent::FromBehavior(event),
        );
    }

    pub fn send_to_net(&self, route: CrossHandlerRoute, msg: ConnectionMsg<MSG>) {
        self.cross_gate
            .read()
            .send_to_net(self.service_id, route, msg);
    }

    pub fn close_conn(&self, conn: u32) {
        self.cross_gate.read().close_conn(conn);
    }

    pub fn close_peer(&self, peer: PeerId) {
        self.cross_gate.read().close_peer(peer);
    }
}

pub struct ConnectionAgent<BE, HE, MSG> {
    service_id: u8,
    local_peer_id: PeerId,
    remote_peer_id: PeerId,
    conn_id: u32,
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
    fn new(
        service_id: u8,
        local_peer_id: PeerId,
        remote_peer_id: PeerId,
        conn_id: u32,
        sender: Arc<dyn ConnectionSender<MSG>>,
        internal_tx: Sender<NetworkPlaneInternalEvent<BE, MSG>>,
        cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
    ) -> Self {
        Self {
            service_id,
            local_peer_id,
            remote_peer_id,
            conn_id,
            sender,
            internal_tx,
            cross_gate,
        }
    }

    pub fn conn_id(&self) -> u32 {
        self.conn_id
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub fn remote_peer_id(&self) -> PeerId {
        self.remote_peer_id
    }

    pub fn send_behavior(&self, event: BE) {
        match self
            .internal_tx
            .send_blocking(NetworkPlaneInternalEvent::ToBehaviour {
                service_id: self.service_id,
                peer_id: self.remote_peer_id,
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
        self.sender.send(self.service_id, msg);
    }

    pub fn send_to_handler(&self, route: CrossHandlerRoute, event: HE) {
        self.cross_gate.read().send_to_handler(
            self.service_id,
            route,
            CrossHandlerEvent::FromHandler(self.remote_peer_id, self.conn_id, event),
        );
    }

    pub fn send_to_net(&self, route: CrossHandlerRoute, msg: ConnectionMsg<MSG>) {
        self.cross_gate
            .read()
            .send_to_net(self.service_id, route, msg);
    }

    pub fn close_conn(&self) {
        self.cross_gate.read().close_conn(self.conn_id);
    }
}

enum NetworkPlaneInternalEvent<BE, MSG> {
    ToBehaviour {
        service_id: u8,
        peer_id: PeerId,
        conn_id: u32,
        event: BE,
    },
    IncomingDisconnected(Arc<dyn ConnectionSender<MSG>>),
    OutgoingDisconnected(Arc<dyn ConnectionSender<MSG>>),
}

pub struct NetworkPlaneConfig<BE, HE, MSG> {
    /// Local node peer_id, which is u32 value
    pub local_peer_id: PeerId,
    /// Tick_ms, each tick_ms miliseconds, network will call tick function on both behavior and handler
    pub tick_ms: u64,
    /// List of behavior
    pub behavior: Vec<Box<dyn NetworkBehavior<BE, HE, MSG> + Send + Sync>>,
    /// Transport which is used
    pub transport: Box<dyn Transport<MSG> + Send + Sync>,
    /// Timer for getting timestamp miliseconds
    pub timer: Arc<dyn Timer>,
}

pub struct NetworkPlane<BE, HE, MSG> {
    local_peer_id: PeerId,
    tick_ms: u64,
    behaviors: Vec<
        Option<(
            Box<dyn NetworkBehavior<BE, HE, MSG> + Send + Sync>,
            BehaviorAgent<HE, MSG>,
        )>,
    >,
    transport: Box<dyn Transport<MSG> + Send + Sync>,
    timer: Arc<dyn Timer>,
    internal_tx: Sender<NetworkPlaneInternalEvent<BE, MSG>>,
    internal_rx: Receiver<NetworkPlaneInternalEvent<BE, MSG>>,
    cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>>,
    tick_interval: Interval,
}

impl<BE, HE, MSG> NetworkPlane<BE, HE, MSG>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
    MSG: Send + Sync + 'static,
{
    /// Creating new network plane, after create need to run
    /// `while let Some(_) = plane.run().await {}`
    pub fn new(conf: NetworkPlaneConfig<BE, HE, MSG>) -> Self {
        let cross_gate: Arc<RwLock<CrossHandlerGate<HE, MSG>>> = Default::default();

        let (internal_tx, internal_rx) = unbounded();
        let mut behaviors: Vec<
            Option<(
                Box<dyn NetworkBehavior<BE, HE, MSG> + Send + Sync>,
                BehaviorAgent<HE, MSG>,
            )>,
        > = init_vec(256, || None);

        for behavior in conf.behavior {
            let service_id = behavior.service_id() as usize;
            if behaviors[service_id].is_none() {
                behaviors[service_id] = Some((
                    behavior,
                    BehaviorAgent::new(
                        service_id as u8,
                        conf.local_peer_id,
                        conf.transport.connector(),
                        cross_gate.clone(),
                    ),
                ));
            } else {
                panic!("Duplicate service {}", behavior.service_id())
            }
        }

        Self {
            local_peer_id: conf.local_peer_id,
            tick_ms: conf.tick_ms,
            behaviors,
            transport: conf.transport,
            tick_interval: async_std::stream::interval(Duration::from_millis(conf.tick_ms)),
            internal_tx,
            internal_rx,
            timer: conf.timer,
            cross_gate,
        }
    }

    fn process_transport_event(&mut self, e: Result<TransportEvent<MSG>, ()>) -> Result<(), ()> {
        let (outgoing, sender, mut receiver, mut handlers, mut conn_internal_rx) = match e? {
            TransportEvent::Incoming(sender, receiver) => {
                log::info!(
                    "[NetworkPlane] received TransportEvent::Incoming({}, {})",
                    receiver.remote_peer_id(),
                    receiver.connection_id()
                );
                let mut cross_gate = self.cross_gate.write();
                let rx = cross_gate.add_conn(sender.clone());
                drop(cross_gate);
                if let Some(rx) = rx {
                    let mut handlers: Vec<
                        Option<(
                            Box<dyn ConnectionHandler<BE, HE, MSG>>,
                            ConnectionAgent<BE, HE, MSG>,
                        )>,
                    > = init_vec(256, || None);
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            let conn_agent = ConnectionAgent::<BE, HE, MSG>::new(
                                behaviour.service_id(),
                                self.local_peer_id,
                                receiver.remote_peer_id(),
                                receiver.connection_id(),
                                sender.clone(),
                                self.internal_tx.clone(),
                                self.cross_gate.clone(),
                            );
                            handlers[behaviour.service_id() as usize] = behaviour
                                .on_incoming_connection_connected(agent, sender.clone())
                                .map(|h| (h, conn_agent));
                        }
                    }
                    (false, sender, receiver, handlers, rx)
                } else {
                    return Ok(());
                }
            }
            TransportEvent::Outgoing(sender, receiver) => {
                log::info!(
                    "[NetworkPlane] received TransportEvent::Outgoing({}, {})",
                    receiver.remote_peer_id(),
                    receiver.connection_id()
                );
                let mut cross_gate = self.cross_gate.write();
                let rx = cross_gate.add_conn(sender.clone());
                drop(cross_gate);
                if let Some(rx) = rx {
                    let mut handlers: Vec<
                        Option<(
                            Box<dyn ConnectionHandler<BE, HE, MSG>>,
                            ConnectionAgent<BE, HE, MSG>,
                        )>,
                    > = init_vec(256, || None);
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            let conn_agent = ConnectionAgent::<BE, HE, MSG>::new(
                                behaviour.service_id(),
                                self.local_peer_id,
                                receiver.remote_peer_id(),
                                receiver.connection_id(),
                                sender.clone(),
                                self.internal_tx.clone(),
                                self.cross_gate.clone(),
                            );
                            handlers[behaviour.service_id() as usize] = behaviour
                                .on_outgoing_connection_connected(agent, sender.clone())
                                .map(|h| (h, conn_agent));
                        }
                    }
                    (true, sender, receiver, handlers, rx)
                } else {
                    log::warn!("[NetworkPlane] received TransportEvent::Outgoing but cannot add to cross_gate");
                    return Ok(());
                }
            }
            TransportEvent::OutgoingError {
                peer_id,
                connection_id,
                err,
            } => {
                log::info!(
                    "[NetworkPlane] received TransportEvent::OutgoingError({}, {})",
                    peer_id,
                    connection_id
                );
                for behaviour in &mut self.behaviors {
                    if let Some((behaviour, agent)) = behaviour {
                        behaviour.on_outgoing_connection_error(agent, peer_id, connection_id, &err);
                    }
                }
                return Ok(());
            }
        };

        let internal_tx = self.internal_tx.clone();
        let tick_ms = self.tick_ms;
        let timer = self.timer.clone();
        async_std::task::spawn(async move {
            log::info!(
                "[NetworkPlane] fire handlers on_opened ({}, {})",
                receiver.remote_peer_id(),
                receiver.connection_id()
            );
            for handler in &mut handlers {
                if let Some((handler, conn_agent)) = handler {
                    handler.on_opened(conn_agent);
                }
            }
            let mut tick_interval = async_std::stream::interval(Duration::from_millis(tick_ms));
            loop {
                select! {
                    e = tick_interval.next().fuse() => {
                        let ts_ms = timer.now_ms();
                        for handler in &mut handlers {
                            if let Some((handler, conn_agent)) = handler {
                                handler.on_tick(conn_agent, ts_ms, tick_ms);
                            }
                        }
                    }
                    e = conn_internal_rx.recv().fuse() => {
                        match e {
                            Ok((service_id, event)) => match event {
                                CrossHandlerEvent::FromBehavior(e) => {
                                    log::debug!("[NetworkPlane] fire handlers on_behavior_event for conn ({}, {}) from service {}", receiver.remote_peer_id(), receiver.connection_id(), service_id);
                                    if let Some((handler, conn_agent)) = &mut handlers[service_id as usize] {
                                        handler.on_behavior_event(&conn_agent, e);
                                    } else {
                                        debug_assert!(false, "service not found {}", service_id);
                                    }
                                },
                                CrossHandlerEvent::FromHandler(peer, conn, e) => {
                                    log::debug!("[NetworkPlane] fire handlers on_other_handler_event for conn ({}, {}) from service {}", receiver.remote_peer_id(), receiver.connection_id(), service_id);
                                    if let Some((handler, conn_agent)) = &mut handlers[service_id as usize] {
                                        handler.on_other_handler_event(&conn_agent, peer, conn, e);
                                    } else {
                                        debug_assert!(false, "service not found {}", service_id);
                                    }
                                }
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                    e = receiver.poll().fuse() => match e {
                        Ok(event) => {
                            match &event {
                                ConnectionEvent::Msg { service_id, .. } => {
                                    log::debug!("[NetworkPlane] fire handlers on_event network msg for conn ({}, {}) from service {}", receiver.remote_peer_id(), receiver.connection_id(), service_id);
                                    if let Some((handler, conn_agent)) = &mut handlers[*service_id as usize] {
                                        handler.on_event(&conn_agent, event);
                                    } else {
                                        debug_assert!(false, "service not found {}", service_id);
                                    }
                                }
                                ConnectionEvent::Stats(stats) => {
                                    log::debug!("[NetworkPlane] fire handlers on_event network stats for conn ({}, {})", receiver.remote_peer_id(), receiver.connection_id());
                                    for handler in &mut handlers {
                                        if let Some((handler, conn_agent)) = handler {
                                            handler.on_event(&conn_agent, ConnectionEvent::Stats(stats.clone()));
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            break;
                        }
                    }
                }
            }
            log::info!(
                "[NetworkPlane] fire handlers on_closed ({}, {})",
                receiver.remote_peer_id(),
                receiver.connection_id()
            );
            for handler in &mut handlers {
                if let Some((handler, conn_agent)) = handler {
                    handler.on_closed(&conn_agent);
                }
            }
            if outgoing {
                if let Err(err) = internal_tx
                    .send(NetworkPlaneInternalEvent::IncomingDisconnected(sender))
                    .await
                {
                    log::error!("Sending IncomingDisconnected error {:?}", err);
                }
            } else {
                if let Err(err) = internal_tx
                    .send(NetworkPlaneInternalEvent::OutgoingDisconnected(sender))
                    .await
                {
                    log::error!("Sending OutgoingDisconnected error {:?}", err);
                }
            }
        });

        Ok(())
    }

    /// Run loop for plane which handle tick and connection
    pub async fn run(&mut self) -> Result<(), ()> {
        log::debug!("[NetworkPlane] waiting event");
        select! {
            e = self.tick_interval.next().fuse() => {
                let ts_ms = self.timer.now_ms();
                for behaviour in &mut self.behaviors {
                    if let Some((behaviour, agent)) = behaviour {
                        behaviour.on_tick(agent, ts_ms, self.tick_ms);
                    }
                }
                Ok(())
            }
            e = self.transport.recv().fuse() => {
                self.process_transport_event(e)
            }
            e =  self.internal_rx.recv().fuse() => match e {
                Ok(NetworkPlaneInternalEvent::IncomingDisconnected(sender)) => {
                    log::info!("[NetworkPlane] received NetworkPlaneInternalEvent::IncomingDisconnected({}, {})", sender.remote_peer_id(), sender.connection_id());
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            behaviour.on_incoming_connection_disconnected(agent, sender.clone());
                        }
                    }
                    Ok(())
                },
                Ok(NetworkPlaneInternalEvent::OutgoingDisconnected(sender)) => {
                    log::info!("[NetworkPlane] received NetworkPlaneInternalEvent::OutgoingDisconnected({}, {})", sender.remote_peer_id(), sender.connection_id());
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            behaviour.on_outgoing_connection_disconnected(agent, sender.clone());
                        }
                    }
                    Ok(())
                },
                Ok(NetworkPlaneInternalEvent::ToBehaviour { service_id, peer_id, conn_id, event }) => {
                    log::debug!("[NetworkPlane] received NetworkPlaneInternalEvent::ToBehaviour service: {}, from peer: {} conn_id: {}", service_id, peer_id, conn_id);
                    if let Some((behaviour, agent)) = &mut self.behaviors[service_id as usize] {
                        behaviour.on_handler_event(agent, peer_id, conn_id, event);
                    } else {
                        debug_assert!(false, "service not found {}", service_id);
                    }
                    Ok(())
                },
                Err(_) => {
                    Err(())
                }
            }
        }
    }
}

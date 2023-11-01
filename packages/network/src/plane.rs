mod single_conn;

use crate::behaviour::{ConnectionContext, NetworkBehavior, NetworkBehaviorAction};
use crate::msg::TransportMsg;
use crate::transport::Transport;
use async_std::channel::{unbounded, Receiver, Sender};
use async_std::stream::Interval;
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::RouterTable;
use futures::{select, FutureExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use utils::awaker::Awaker;
use utils::error_handle::ErrorUtils;
use utils::Timer;

use self::bus::PlaneBus;
use self::bus_impl::PlaneBusImpl;
use self::internal::{PlaneInternal, PlaneInternalAction};
use self::single_conn::{PlaneSingleConn, PlaneSingleConnInternal};

pub(crate) mod bus;
mod bus_impl;
mod internal;

struct BehaviourAwake<BE, HE> {
    service_id: u8,
    bus: Arc<PlaneBusImpl<BE, HE>>,
}

impl<BE: Send + Sync + 'static, HE: Send + Sync + 'static> Awaker for BehaviourAwake<BE, HE> {
    fn notify(&self) {
        self.bus.awake_behaviour(self.service_id);
    }

    fn pop_awake_count(&self) -> usize {
        0
    }
}

struct HandlerAwake<BE, HE> {
    service_id: u8,
    conn_id: ConnId,
    bus: Arc<PlaneBusImpl<BE, HE>>,
}

impl<BE: Send + Sync + 'static, HE: Send + Sync + 'static> Awaker for HandlerAwake<BE, HE> {
    fn notify(&self) {
        self.bus.awake_handler(self.service_id, self.conn_id);
    }

    fn pop_awake_count(&self) -> usize {
        0
    }
}

pub enum NetworkPlaneInternalEvent<BE> {
    AwakeBehaviour { service_id: u8 },
    ToBehaviourFromHandler { service_id: u8, node_id: NodeId, conn_id: ConnId, event: BE },
    ToBehaviourLocalEvent { service_id: u8, event: BE },
    ToBehaviourLocalMsg { service_id: u8, msg: TransportMsg },
    IncomingDisconnected(NodeId, ConnId),
    OutgoingDisconnected(NodeId, ConnId),
}

pub enum NetworkPlaneError {
    TransportError,
    InternalQueueError,
    RuntimeError,
}

pub struct NetworkPlaneConfig<BE, HE> {
    /// Node_id, which is u32 value
    pub node_id: NodeId,
    /// Tick_ms, each tick_ms miliseconds, network will call tick function on both behavior and handler
    pub tick_ms: u64,
    /// List of behavior
    pub behaviors: Vec<Box<dyn NetworkBehavior<BE, HE> + Send + Sync>>,
    /// Transport which is used
    pub transport: Box<dyn Transport + Send + Sync>,
    /// Timer for getting timestamp miliseconds
    pub timer: Arc<dyn Timer>,
    /// Routing table, which is used to route message to correct node
    pub router: Arc<dyn RouterTable>,
}

pub struct NetworkPlane<BE, HE> {
    node_id: NodeId,
    tick_ms: u64,
    transport: Box<dyn Transport + Send + Sync>,
    timer: Arc<dyn Timer>,
    router: Arc<dyn RouterTable>,
    internal_tx: Sender<NetworkPlaneInternalEvent<BE>>,
    internal_rx: Receiver<NetworkPlaneInternalEvent<BE>>,
    bus: Arc<PlaneBusImpl<BE, HE>>,
    tick_interval: Interval,
    internal: PlaneInternal<BE, HE>,
}

impl<BE, HE> NetworkPlane<BE, HE>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    /// Creating new network plane, after create need to run
    /// `while let Some(_) = plane.run().await {}`
    pub fn new(conf: NetworkPlaneConfig<BE, HE>) -> Self {
        let (internal_tx, internal_rx) = unbounded();
        let bus: Arc<PlaneBusImpl<BE, HE>> = Arc::new(PlaneBusImpl::new(conf.router.clone(), internal_tx.clone()));

        let mut new_behaviours = vec![];
        for (service_id, behaviour) in conf.behaviors.into_iter().enumerate() {
            let awake = BehaviourAwake {
                service_id: service_id as u8,
                bus: bus.clone(),
            };
            new_behaviours.push((behaviour, Arc::new(awake) as Arc<dyn Awaker>));
        }

        Self {
            node_id: conf.node_id,
            tick_ms: conf.tick_ms,
            transport: conf.transport,
            tick_interval: async_std::stream::interval(Duration::from_millis(conf.tick_ms)),
            internal_tx,
            internal_rx,
            timer: conf.timer,
            router: conf.router,
            internal: PlaneInternal::new(conf.node_id, new_behaviours),
            bus,
        }
    }

    pub fn started(&mut self) {
        self.internal.started(self.timer.now_ms());
        self.pop_actions(self.timer.now_ms());
    }

    /// Run loop for plane which handle tick and connection
    pub async fn recv(&mut self) -> Result<(), NetworkPlaneError> {
        log::trace!("[NetworkPlane {}] waiting event", self.node_id);
        let res = select! {
            _ = self.tick_interval.next().fuse() => {
                self.internal.on_tick(self.timer.now_ms(), self.tick_ms);
                Ok(())
            },
            e = self.transport.recv().fuse() => match e {
                Ok(e) => {
                    self.internal.on_transport_event(self.timer.now_ms(), e)
                        .map_err(|_e| NetworkPlaneError::RuntimeError)
                },
                Err(_e) => {
                    Err(NetworkPlaneError::TransportError)
                }
            },
            e =  self.internal_rx.recv().fuse() => match e {
                Ok(event) => {
                    self.internal.on_internal_event(self.timer.now_ms(), event)
                        .map_err(|_e| NetworkPlaneError::RuntimeError)
                },
                Err(_) => {
                    Err(NetworkPlaneError::InternalQueueError)
                }
            }
        };
        self.pop_actions(self.timer.now_ms());
        res
    }

    pub fn stopped(&mut self) {
        log::info!("[NetworkPlane {}] stopped", self.node_id);
        self.internal.stopped(self.timer.now_ms());
        self.pop_actions(self.timer.now_ms());
    }

    fn pop_actions(&mut self, now_ms: u64) {
        while let Some(action) = self.internal.pop_action() {
            match action {
                PlaneInternalAction::SpawnConnection { outgoing, sender, receiver, handlers } => {
                    let internal_tx = self.internal_tx.clone();
                    let tick_ms = self.tick_ms;
                    let timer = self.timer.clone();
                    let router = self.router.clone();
                    let bus = self.bus.clone();
                    if let Some(conn_internal_rx) = bus.add_conn(sender.clone()) {
                        let mut new_handlers = vec![];
                        for (service_id, handler) in handlers.into_iter().enumerate() {
                            if let Some(handler) = handler {
                                let context = ConnectionContext {
                                    service_id: service_id as u8,
                                    local_node_id: self.node_id,
                                    remote_node_id: sender.remote_node_id(),
                                    conn_id: sender.conn_id(),
                                    awaker: Arc::new(HandlerAwake {
                                        bus: self.bus.clone(),
                                        service_id: service_id as u8,
                                        conn_id: sender.conn_id(),
                                    }),
                                };
                                new_handlers.push(Some((handler, context)));
                            } else {
                                new_handlers.push(None);
                            }
                        }

                        async_std::task::spawn(async move {
                            let remote_node_id = sender.remote_node_id();
                            let conn_id = sender.conn_id();

                            let mut single_conn = PlaneSingleConn {
                                sender,
                                receiver,
                                bus_rx: conn_internal_rx,
                                tick_ms,
                                tick_interval: async_std::stream::interval(Duration::from_millis(tick_ms)),
                                timer,
                                router,
                                bus: bus.clone(),
                                internal: PlaneSingleConnInternal { handlers: new_handlers },
                            };
                            single_conn.start();
                            while let Ok(_) = single_conn.recv().await {}
                            single_conn.end();
                            if bus.remove_conn(remote_node_id, conn_id).is_none() {
                                log::warn!("[NetworkPlane] remove conn ({}, {}) failed", remote_node_id, conn_id);
                            } else {
                                if outgoing {
                                    internal_tx
                                        .send(NetworkPlaneInternalEvent::OutgoingDisconnected(remote_node_id, conn_id))
                                        .await
                                        .print_error("Should send disconnect event");
                                } else {
                                    internal_tx
                                        .send(NetworkPlaneInternalEvent::IncomingDisconnected(remote_node_id, conn_id))
                                        .await
                                        .print_error("Should send disconnect event");
                                }
                            }
                        });
                    } else {
                        log::warn!("[NetworkPlane] add conn ({}, {}) failed", sender.remote_node_id(), sender.conn_id());
                    }
                }
                PlaneInternalAction::BehaviorAction(service, action) => match action {
                    NetworkBehaviorAction::ConnectTo(local_uuid, node_id, dest) => {
                        if let Err(err) = self.transport.connector().connect_to(local_uuid, node_id, dest) {
                            log::warn!("[NetworkPlane] connect to {} failed: {}", node_id, err);
                            if let Err(e) = self.internal.on_transport_event(
                                now_ms,
                                crate::transport::TransportEvent::OutgoingError {
                                    local_uuid,
                                    node_id,
                                    conn_id: None,
                                    err,
                                },
                            ) {
                                log::error!("[NetworkPlane] on_transport_event error: {:?}", e);
                            }
                        }
                    }
                    NetworkBehaviorAction::ToNet(msg) => {
                        self.bus.to_net(msg);
                    }
                    NetworkBehaviorAction::ToNetConn(conn_id, msg) => {
                        self.bus.to_net_conn(conn_id, msg);
                    }
                    NetworkBehaviorAction::ToNetNode(node, msg) => {
                        self.bus.to_net_node(node, msg);
                    }
                    NetworkBehaviorAction::ToHandler(route, msg) => {
                        self.bus.to_handler(service, route, bus::HandleEvent::FromBehavior(msg));
                    }
                    NetworkBehaviorAction::CloseConn(conn) => {
                        self.bus.close_conn(conn);
                    }
                    NetworkBehaviorAction::CloseNode(node) => {
                        self.bus.close_node(node);
                    }
                },
            }
        }
    }
}

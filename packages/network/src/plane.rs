use crate::behaviour::{ConnectionHandler, NetworkBehavior};
use crate::internal::agent::{BehaviorAgent, ConnectionAgent};
use crate::internal::cross_handler_gate::{CrossHandlerEvent, CrossHandlerGate};
use crate::transport::{
    ConnectionEvent, ConnectionReceiver, ConnectionSender, 
    RpcAnswer, Transport, TransportEvent, TransportRpc,
};
use async_std::channel::{unbounded, Receiver, Sender};
use async_std::stream::Interval;
use bluesea_identity::{ConnId, NodeId};
use bluesea_router::{RouterTable, RouteAction};
use futures::{select, FutureExt, StreamExt};
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use utils::init_vec::init_vec;
use utils::Timer;

pub enum NetworkPlaneInternalEvent<BE> {
    ToBehaviour { service_id: u8, node_id: NodeId, conn_id: ConnId, event: BE },
    IncomingDisconnected(Arc<dyn ConnectionSender>),
    OutgoingDisconnected(Arc<dyn ConnectionSender>),
}

pub struct NetworkPlaneConfig<BE, HE, Req, Res> {
    /// Local node_id, which is u32 value
    pub local_node_id: NodeId,
    /// Tick_ms, each tick_ms miliseconds, network will call tick function on both behavior and handler
    pub tick_ms: u64,
    /// List of behavior
    pub behavior: Vec<Box<dyn NetworkBehavior<BE, HE, Req, Res> + Send + Sync>>,
    /// Transport which is used
    pub transport: Box<dyn Transport + Send + Sync>,
    pub transport_rpc: Box<dyn TransportRpc<Req, Res> + Send + Sync>,
    /// Timer for getting timestamp miliseconds
    pub timer: Arc<dyn Timer>,
    /// Routing table, which is used to route message to correct node
    pub router: Arc<dyn RouterTable>,
}

pub struct NetworkPlane<BE, HE, Req, Res> {
    local_node_id: NodeId,
    tick_ms: u64,
    behaviors: Vec<Option<(Box<dyn NetworkBehavior<BE, HE, Req, Res> + Send + Sync>, BehaviorAgent<HE>)>>,
    transport: Box<dyn Transport + Send + Sync>,
    transport_rpc: Box<dyn TransportRpc<Req, Res> + Send + Sync>,
    timer: Arc<dyn Timer>,
    router: Arc<dyn RouterTable>,
    internal_tx: Sender<NetworkPlaneInternalEvent<BE>>,
    internal_rx: Receiver<NetworkPlaneInternalEvent<BE>>,
    cross_gate: Arc<RwLock<CrossHandlerGate<HE>>>,
    tick_interval: Interval,
}

impl<BE, HE, Req, Res> NetworkPlane<BE, HE, Req, Res>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
    Req: Send + Sync + 'static,
    Res: Send + Sync + 'static,
{
    /// Creating new network plane, after create need to run
    /// `while let Some(_) = plane.run().await {}`
    pub fn new(conf: NetworkPlaneConfig<BE, HE, Req, Res>) -> Self {
        let cross_gate: Arc<RwLock<CrossHandlerGate<HE>>> = Arc::new(RwLock::new(CrossHandlerGate::new(conf.router.clone())));

        let (internal_tx, internal_rx) = unbounded();
        let mut behaviors: Vec<Option<(Box<dyn NetworkBehavior<BE, HE, Req, Res> + Send + Sync>, BehaviorAgent<HE>)>> = init_vec(256, || None);

        for behavior in conf.behavior {
            let service_id = behavior.service_id() as usize;
            if behaviors[service_id].is_none() {
                behaviors[service_id] = Some((behavior, BehaviorAgent::new(service_id as u8, conf.local_node_id, conf.transport.connector(), cross_gate.clone())));
            } else {
                panic!("Duplicate service {}", behavior.service_id())
            }
        }

        Self {
            local_node_id: conf.local_node_id,
            tick_ms: conf.tick_ms,
            behaviors,
            transport: conf.transport,
            transport_rpc: conf.transport_rpc,
            tick_interval: async_std::stream::interval(Duration::from_millis(conf.tick_ms)),
            internal_tx,
            internal_rx,
            timer: conf.timer,
            router: conf.router,
            cross_gate,
        }
    }

    fn process_transport_event(&mut self, e: Result<TransportEvent, ()>) -> Result<(), ()> {
        let (outgoing, sender, mut receiver, mut handlers, conn_internal_rx) = match e? {
            TransportEvent::IncomingRequest(node, conn_id, acceptor) => {
                for behaviour in &mut self.behaviors {
                    if let Some((behaviour, _agent)) = behaviour {
                        if let Err(err) = behaviour.check_incoming_connection(node, conn_id) {
                            acceptor.reject(err);
                            return Ok(());
                        }
                    }
                }
                acceptor.accept();
                return Ok(());
            }
            TransportEvent::OutgoingRequest(node, conn_id, acceptor) => {
                for behaviour in &mut self.behaviors {
                    if let Some((behaviour, _agent)) = behaviour {
                        if let Err(err) = behaviour.check_outgoing_connection(node, conn_id) {
                            acceptor.reject(err);
                            return Ok(());
                        }
                    }
                }
                acceptor.accept();
                return Ok(());
            }
            TransportEvent::Incoming(sender, receiver) => {
                log::info!("[NetworkPlane] received TransportEvent::Incoming({}, {})", receiver.remote_node_id(), receiver.conn_id());
                let mut cross_gate = self.cross_gate.write();
                let rx = cross_gate.add_conn(sender.clone());
                drop(cross_gate);
                if let Some(rx) = rx {
                    let mut handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionAgent<BE, HE>)>> = init_vec(256, || None);
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            let conn_agent = ConnectionAgent::<BE, HE>::new(
                                behaviour.service_id(),
                                self.local_node_id,
                                receiver.remote_node_id(),
                                receiver.conn_id(),
                                sender.clone(),
                                self.internal_tx.clone(),
                                self.cross_gate.clone(),
                            );
                            handlers[behaviour.service_id() as usize] = behaviour.on_incoming_connection_connected(agent, sender.clone()).map(|h| (h, conn_agent));
                        }
                    }
                    (false, sender, receiver, handlers, rx)
                } else {
                    return Ok(());
                }
            }
            TransportEvent::Outgoing(sender, receiver) => {
                log::info!("[NetworkPlane] received TransportEvent::Outgoing({}, {})", receiver.remote_node_id(), receiver.conn_id());
                let mut cross_gate = self.cross_gate.write();
                let rx = cross_gate.add_conn(sender.clone());
                drop(cross_gate);
                if let Some(rx) = rx {
                    let mut handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionAgent<BE, HE>)>> = init_vec(256, || None);
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            let conn_agent = ConnectionAgent::<BE, HE>::new(
                                behaviour.service_id(),
                                self.local_node_id,
                                receiver.remote_node_id(),
                                receiver.conn_id(),
                                sender.clone(),
                                self.internal_tx.clone(),
                                self.cross_gate.clone(),
                            );
                            handlers[behaviour.service_id() as usize] = behaviour.on_outgoing_connection_connected(agent, sender.clone()).map(|h| (h, conn_agent));
                        }
                    }
                    (true, sender, receiver, handlers, rx)
                } else {
                    log::warn!("[NetworkPlane] received TransportEvent::Outgoing but cannot add to cross_gate");
                    return Ok(());
                }
            }
            TransportEvent::OutgoingError {
                node_id,
                conn_id,
                err,
            } => {
                log::info!("[NetworkPlane] received TransportEvent::OutgoingError({}, {})", node_id, conn_id);
                for behaviour in &mut self.behaviors {
                    if let Some((behaviour, agent)) = behaviour {
                        behaviour.on_outgoing_connection_error(agent, node_id, conn_id, &err);
                    }
                }
                return Ok(());
            }
        };

        let internal_tx = self.internal_tx.clone();
        let tick_ms = self.tick_ms;
        let timer = self.timer.clone();
        let router = self.router.clone();
        async_std::task::spawn(async move {
            log::info!("[NetworkPlane] fire handlers on_opened ({}, {})", receiver.remote_node_id(), receiver.conn_id());
            for handler in &mut handlers {
                if let Some((handler, conn_agent)) = handler {
                    handler.on_opened(conn_agent);
                }
            }
            let mut tick_interval = async_std::stream::interval(Duration::from_millis(tick_ms));
            loop {
                select! {
                    _ = tick_interval.next().fuse() => {
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
                                    log::debug!("[NetworkPlane] fire handlers on_behavior_event for conn ({}, {}) from service {}", receiver.remote_node_id(), receiver.conn_id(), service_id);
                                    if let Some((handler, conn_agent)) = &mut handlers[service_id as usize] {
                                        handler.on_behavior_event(&conn_agent, e);
                                    } else {
                                        debug_assert!(false, "service not found {}", service_id);
                                    }
                                },
                                CrossHandlerEvent::FromHandler(node, conn, e) => {
                                    log::debug!("[NetworkPlane] fire handlers on_other_handler_event for conn ({}, {}) from service {}", receiver.remote_node_id(), receiver.conn_id(), service_id);
                                    if let Some((handler, conn_agent)) = &mut handlers[service_id as usize] {
                                        handler.on_other_handler_event(&conn_agent, node, conn, e);
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
                            process_conn_msg(event, &mut handlers, &sender, &receiver, &router);
                        }
                        Err(err) => {
                            log::error!("{:?}", err);
                            break;
                        }
                    }
                }
            }
            log::info!("[NetworkPlane] fire handlers on_closed ({}, {})", receiver.remote_node_id(), receiver.conn_id());
            for handler in &mut handlers {
                if let Some((handler, conn_agent)) = handler {
                    handler.on_closed(&conn_agent);
                }
            }
            if outgoing {
                if let Err(err) = internal_tx.send(NetworkPlaneInternalEvent::IncomingDisconnected(sender)).await {
                    log::error!("Sending IncomingDisconnected error {:?}", err);
                }
            } else {
                if let Err(err) = internal_tx.send(NetworkPlaneInternalEvent::OutgoingDisconnected(sender)).await {
                    log::error!("Sending OutgoingDisconnected error {:?}", err);
                }
            }
        });

        Ok(())
    }

    fn process_transport_rpc_event(&mut self, e: Result<(u8, Req, Box<dyn RpcAnswer<Res>>), ()>) -> Result<(), ()> {
        let (service_id, req, res) = e?;
        if let Some(Some((behaviour, agent))) = self.behaviors.get_mut(service_id as usize) {
            behaviour.on_rpc(agent, req, res);
            Ok(())
        } else {
            res.error(0, "SERVICE_NOT_FOUND");
            Err(())
        }
    }

    /// Run loop for plane which handle tick and connection
    pub async fn recv(&mut self) -> Result<(), ()> {
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
                    log::info!("[NetworkPlane] received NetworkPlaneInternalEvent::IncomingDisconnected({}, {})", sender.remote_node_id(), sender.conn_id());
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            behaviour.on_incoming_connection_disconnected(agent, sender.clone());
                        }
                    }
                    Ok(())
                },
                Ok(NetworkPlaneInternalEvent::OutgoingDisconnected(sender)) => {
                    log::info!("[NetworkPlane] received NetworkPlaneInternalEvent::OutgoingDisconnected({}, {})", sender.remote_node_id(), sender.conn_id());
                    for behaviour in &mut self.behaviors {
                        if let Some((behaviour, agent)) = behaviour {
                            behaviour.on_outgoing_connection_disconnected(agent, sender.clone());
                        }
                    }
                    Ok(())
                },
                Ok(NetworkPlaneInternalEvent::ToBehaviour { service_id, node_id, conn_id, event }) => {
                    log::debug!("[NetworkPlane] received NetworkPlaneInternalEvent::ToBehaviour service: {}, from node: {} conn_id: {}", service_id, node_id, conn_id);
                    if let Some((behaviour, agent)) = &mut self.behaviors[service_id as usize] {
                        behaviour.on_handler_event(agent, node_id, conn_id, event);
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

fn process_conn_msg<BE, HE>(
    event: ConnectionEvent,
    handlers: &mut Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionAgent<BE, HE>)>>,
    _sender: &Arc<dyn ConnectionSender>,
    receiver: &Box<dyn ConnectionReceiver + Send>,
    router: &Arc<dyn RouterTable>,
) {
    match &event {
        ConnectionEvent::Msg(msg) => match router.path_to(&msg.header.route, msg.header.service_id) {
            RouteAction::Reject => {}
            RouteAction::Local => {
                log::debug!(
                    "[NetworkPlane] fire handlers on_event network msg for conn ({}, {}) from service {}",
                    receiver.remote_node_id(),
                    receiver.conn_id(),
                    msg.header.service_id
                );
                if let Some((handler, conn_agent)) = &mut handlers[msg.header.service_id as usize] {
                    handler.on_event(&conn_agent, event);
                } else {
                    debug_assert!(false, "service not found {}", msg.header.service_id);
                }
            }
            RouteAction::Next(_conn, _node_id) => {
                //TODO   
            }
        },
        ConnectionEvent::Stats(stats) => {
            log::debug!("[NetworkPlane] fire handlers on_event network stats for conn ({}, {})", receiver.remote_node_id(), receiver.conn_id());
            for handler in handlers {
                if let Some((handler, conn_agent)) = handler {
                    handler.on_event(&conn_agent, ConnectionEvent::Stats(stats.clone()));
                }
            }
        }
    }
}

use std::{collections::VecDeque, sync::Arc};

use bluesea_identity::NodeId;
use utils::init_vec::init_vec;

use crate::{
    behaviour::{BehaviorContext, ConnectionContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    transport::{ConnectionReceiver, ConnectionSender, TransportEvent},
};

use super::NetworkPlaneInternalEvent;

pub enum PlaneInternalError {
    InvalidServiceId(u8),
}

pub enum PlaneInternalAction<BE, HE> {
    SpawnConnection {
        outgoing: bool,
        sender: Arc<dyn ConnectionSender>,
        receiver: Box<dyn ConnectionReceiver + Send>,
        handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionContext)>>,
    },
    BehaviorAction(u8, NetworkBehaviorAction<HE>),
}

pub struct PlaneInternal<BE, HE> {
    node_id: NodeId,
    action_queue: VecDeque<PlaneInternalAction<BE, HE>>,
    behaviors: Vec<Option<(Box<dyn NetworkBehavior<BE, HE> + Send + Sync>, BehaviorContext)>>,
}

impl<BE, HE> PlaneInternal<BE, HE> {
    pub fn new(node_id: NodeId, conf_behaviors: Vec<Box<dyn NetworkBehavior<BE, HE> + Send + Sync>>) -> Self {
        let mut behaviors: Vec<Option<(Box<dyn NetworkBehavior<BE, HE> + Send + Sync>, BehaviorContext)>> = init_vec(256, || None);

        for behavior in conf_behaviors {
            let service_id = behavior.service_id() as usize;
            if behaviors[service_id].is_none() {
                behaviors[service_id] = Some((behavior, BehaviorContext::new(service_id as u8, node_id)));
            } else {
                panic!("Duplicate service {}", behavior.service_id())
            }
        }

        Self {
            node_id,
            action_queue: Default::default(),
            behaviors,
        }
    }

    pub fn started(&mut self, now_ms: u64) {
        log::info!("[NetworkPlane {}] started", self.node_id);
        for (behaviour, agent) in self.behaviors.iter_mut().flatten() {
            behaviour.on_started(agent, now_ms);
        }

        self.pop_behaviours_action();
    }

    pub fn on_tick(&mut self, now_ms: u64, interval_ms: u64) {
        for (behaviour, context) in self.behaviors.iter_mut().flatten() {
            behaviour.on_tick(context, now_ms, interval_ms);
        }

        self.pop_behaviours_action();
    }

    pub fn on_internal_event(&mut self, now_ms: u64, event: NetworkPlaneInternalEvent<BE>) -> Result<(), PlaneInternalError> {
        let res = match event {
            NetworkPlaneInternalEvent::IncomingDisconnected(node_id, conn_id) => {
                log::info!("[NetworkPlane {}] received NetworkPlaneInternalEvent::IncomingDisconnected({}, {})", self.node_id, node_id, conn_id);
                for (behaviour, context) in self.behaviors.iter_mut().flatten() {
                    behaviour.on_incoming_connection_disconnected(context, now_ms, node_id, conn_id);
                }
                Ok(())
            }
            NetworkPlaneInternalEvent::OutgoingDisconnected(node_id, conn_id) => {
                log::info!("[NetworkPlane {}] received NetworkPlaneInternalEvent::OutgoingDisconnected({}, {})", self.node_id, node_id, conn_id);
                for (behaviour, context) in self.behaviors.iter_mut().flatten() {
                    behaviour.on_outgoing_connection_disconnected(context, now_ms, node_id, conn_id);
                }
                Ok(())
            }
            NetworkPlaneInternalEvent::ToBehaviourFromHandler { service_id, node_id, conn_id, event } => {
                log::debug!(
                    "[NetworkPlane {}] received NetworkPlaneInternalEvent::ToBehaviour service: {}, from node: {} conn_id: {}",
                    self.node_id,
                    service_id,
                    node_id,
                    conn_id
                );
                if let Some((behaviour, context)) = &mut self.behaviors[service_id as usize] {
                    behaviour.on_handler_event(context, now_ms, node_id, conn_id, event);
                    Ok(())
                } else {
                    debug_assert!(false, "service not found {}", service_id);
                    Err(PlaneInternalError::InvalidServiceId(service_id))
                }
            }
            NetworkPlaneInternalEvent::ToBehaviourLocalEvent { service_id, event } => {
                log::debug!("[NetworkPlane {}] received NetworkPlaneInternalEvent::ToBehaviourLocalEvent service: {}", self.node_id, service_id);
                if let Some((behaviour, context)) = &mut self.behaviors[service_id as usize] {
                    behaviour.on_local_event(context, event);
                    Ok(())
                } else {
                    debug_assert!(false, "service not found {}", service_id);
                    Err(PlaneInternalError::InvalidServiceId(service_id))
                }
            }
            NetworkPlaneInternalEvent::ToBehaviourLocalMsg { service_id, msg } => {
                log::debug!("[NetworkPlane {}] received NetworkPlaneInternalEvent::ToBehaviourLocalMsg service: {}", self.node_id, service_id);
                if let Some((behaviour, context)) = &mut self.behaviors[service_id as usize] {
                    behaviour.on_local_msg(context, now_ms, msg);
                    Ok(())
                } else {
                    debug_assert!(false, "service not found {}", service_id);
                    Err(PlaneInternalError::InvalidServiceId(service_id))
                }
            }
        };

        self.pop_behaviours_action();
        res
    }

    pub fn on_transport_event(&mut self, now_ms: u64, event: TransportEvent) -> Result<(), PlaneInternalError> {
        match event {
            TransportEvent::IncomingRequest(node, conn_id, acceptor) => {
                let mut rejected = false;
                for (behaviour, context) in self.behaviors.iter_mut().flatten() {
                    if let Err(err) = behaviour.check_incoming_connection(&context, now_ms, node, conn_id) {
                        acceptor.reject(err);
                        rejected = true;
                        break;
                    }
                }
                if !rejected {
                    acceptor.accept();
                }
            }
            TransportEvent::OutgoingRequest(node, conn_id, acceptor, local_uuid) => {
                let mut rejected = false;
                for (behaviour, context) in self.behaviors.iter_mut().flatten() {
                    if let Err(err) = behaviour.check_outgoing_connection(&context, now_ms, node, conn_id, local_uuid) {
                        acceptor.reject(err);
                        rejected = true;
                        break;
                    }
                }
                if !rejected {
                    acceptor.accept();
                }
            }
            TransportEvent::Incoming(sender, receiver) => {
                log::info!(
                    "[NetworkPlane {}] received TransportEvent::Incoming({}, {})",
                    self.node_id,
                    receiver.remote_node_id(),
                    receiver.conn_id()
                );
                let mut handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionContext)>> = init_vec(256, || None);
                for (behaviour, context) in self.behaviors.iter_mut().flatten() {
                    let conn_context = ConnectionContext::new(behaviour.service_id(), self.node_id, receiver.remote_node_id(), receiver.conn_id());
                    handlers[behaviour.service_id() as usize] = behaviour.on_incoming_connection_connected(context, now_ms, sender.clone()).map(|h| (h, conn_context));
                }
                self.action_queue.push_back(PlaneInternalAction::SpawnConnection {
                    outgoing: false,
                    sender,
                    receiver,
                    handlers,
                });
            }
            TransportEvent::Outgoing(sender, receiver, local_uuid) => {
                log::info!(
                    "[NetworkPlane {}] received TransportEvent::Outgoing({}, {})",
                    self.node_id,
                    receiver.remote_node_id(),
                    receiver.conn_id()
                );
                let mut handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionContext)>> = init_vec(256, || None);
                for (behaviour, context) in self.behaviors.iter_mut().flatten() {
                    let conn_context = ConnectionContext::new(behaviour.service_id(), self.node_id, receiver.remote_node_id(), receiver.conn_id());
                    handlers[behaviour.service_id() as usize] = behaviour.on_outgoing_connection_connected(context, now_ms, sender.clone(), local_uuid).map(|h| (h, conn_context));
                }
                self.action_queue.push_back(PlaneInternalAction::SpawnConnection {
                    outgoing: true,
                    sender,
                    receiver,
                    handlers,
                });
            }
            TransportEvent::OutgoingError { local_uuid, node_id, conn_id, err } => {
                log::info!("[NetworkPlane {}] received TransportEvent::OutgoingError({}, {:?})", self.node_id, node_id, conn_id);
                for (behaviour, context) in self.behaviors.iter_mut().flatten() {
                    behaviour.on_outgoing_connection_error(context, now_ms, node_id, conn_id, local_uuid, &err);
                }
            }
        }

        self.pop_behaviours_action();
        Ok(())
    }

    pub fn stopped(&mut self, now_ms: u64) {
        log::info!("[NetworkPlane {}] stopped", self.node_id);
        for (behaviour, agent) in self.behaviors.iter_mut().flatten() {
            behaviour.on_stopped(agent, now_ms);
        }

        self.pop_behaviours_action();
    }

    pub fn pop_action(&mut self) -> Option<PlaneInternalAction<BE, HE>> {
        self.action_queue.pop_front()
    }

    fn pop_behaviours_action(&mut self) {
        for (behaviour, context) in self.behaviors.iter_mut().flatten() {
            if let Some(action) = behaviour.pop_action() {
                self.action_queue.push_back(PlaneInternalAction::BehaviorAction(context.service_id, action));
            }
        }
    }
}

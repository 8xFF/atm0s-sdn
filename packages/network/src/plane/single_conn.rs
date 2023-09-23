use std::{time::Duration, sync::Arc};
use async_std::channel::{Receiver, Sender};
use bluesea_router::{RouterTable, RouteAction};
use futures::{select, FutureExt, StreamExt};
use parking_lot::RwLock;
use utils::{Timer, option_handle::OptionUtils};

use crate::{behaviour::ConnectionHandler, ConnectionAgent, transport::{ConnectionReceiver, ConnectionEvent, ConnectionSender}, internal::cross_handler_gate::{CrossHandlerGate, CrossHandlerEvent}, plane::NetworkPlaneInternalEvent};

fn process_conn_msg<BE, HE>(
    event: ConnectionEvent,
    handlers: &mut [Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionAgent<BE, HE>)>],
    _sender: &Arc<dyn ConnectionSender>,
    receiver: &Box<dyn ConnectionReceiver + Send>,
    router: &Arc<dyn RouterTable>,
    cross_gate: &Arc<RwLock<CrossHandlerGate<HE>>>,
) where
    HE: Send + Sync + 'static,
    BE: Send + Sync + 'static,
{
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
                    handler.on_event(conn_agent, event);
                } else {
                    debug_assert!(false, "service not found {}", msg.header.service_id);
                }
            }
            RouteAction::Next(conn, _node_id) => {
                let c_gate = cross_gate.read();
                c_gate.send_to_conn(&conn, msg.clone()).print_none("Should send to conn");
            }
        },
        ConnectionEvent::Stats(stats) => {
            log::debug!("[NetworkPlane] fire handlers on_event network stats for conn ({}, {})", receiver.remote_node_id(), receiver.conn_id());
            for (handler, conn_agent) in handlers.iter_mut().flatten() {
                handler.on_event(conn_agent, ConnectionEvent::Stats(stats.clone()));
            }
        }
    }
}

pub struct PlaneSingleConn<BE, HE> {
    pub(crate) handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionAgent<BE, HE>)>>,
    pub(crate) sender: Arc<dyn ConnectionSender>,
    pub(crate) receiver: Box<dyn ConnectionReceiver + Send>,
    pub(crate) tick_ms: u64,
    pub(crate) timer: Arc<dyn Timer>,
    pub(crate) conn_internal_rx: Receiver<(u8, CrossHandlerEvent<HE>)>,
    pub(crate) internal_tx: Sender<NetworkPlaneInternalEvent<BE>>,
    pub(crate) outgoing: bool,
    pub(crate) router: Arc<dyn RouterTable>,
    pub(crate) cross_gate: Arc<RwLock<CrossHandlerGate<HE>>>,
}

impl<BE, HE> PlaneSingleConn<BE, HE>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
    {
    pub async fn run(&mut self) {
        log::info!("[NetworkPlane] fire handlers on_opened ({}, {})", self.receiver.remote_node_id(), self.receiver.conn_id());

        for (handler, conn_agent) in self.handlers.iter_mut().flatten() {
            handler.on_opened(conn_agent);
        }

        let mut tick_interval = async_std::stream::interval(Duration::from_millis(self.tick_ms));
        loop {
            select! {
                _ = tick_interval.next().fuse() => {
                    let ts_ms = self.timer.now_ms();
                    for (handler, conn_agent) in self.handlers.iter_mut().flatten() {
                        handler.on_tick(conn_agent, ts_ms, self.tick_ms);
                    }
                }
                e = self.conn_internal_rx.recv().fuse() => {
                    match e {
                        Ok((service_id, event)) => match event {
                            CrossHandlerEvent::FromBehavior(e) => {
                                log::debug!("[NetworkPlane] fire handlers on_behavior_event for conn ({}, {}) from service {}", self.receiver.remote_node_id(), self.receiver.conn_id(), service_id);
                                if let Some((handler, conn_agent)) = &mut self.handlers[service_id as usize] {
                                    handler.on_behavior_event(conn_agent, e);
                                } else {
                                    debug_assert!(false, "service not found {}", service_id);
                                }
                            },
                            CrossHandlerEvent::FromHandler(node, conn, e) => {
                                log::debug!("[NetworkPlane] fire handlers on_other_handler_event for conn ({}, {}) from service {}", self.receiver.remote_node_id(), self.receiver.conn_id(), service_id);
                                if let Some((handler, conn_agent)) = &mut self.handlers[service_id as usize] {
                                    handler.on_other_handler_event(conn_agent, node, conn, e);
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
                e = self.receiver.poll().fuse() => match e {
                    Ok(event) => {
                        process_conn_msg(event, &mut self.handlers, &self.sender, &self.receiver, &self.router, &self.cross_gate);
                    }
                    Err(err) => {
                        log::warn!("[NetworkPlane] connection ({}, {}) error {:?}", self.receiver.remote_node_id(), self.receiver.conn_id(), err);
                        break;
                    }
                }
            }
        }
        log::info!("[NetworkPlane] fire handlers on_closed ({}, {})", self.receiver.remote_node_id(), self.receiver.conn_id());
        self.cross_gate.write().remove_conn(self.sender.remote_node_id(), self.sender.conn_id());

        for (handler, conn_agent) in self.handlers.iter_mut().flatten() {
            handler.on_closed(conn_agent);
        }

        if self.outgoing {
            if let Err(err) = self.internal_tx.send(NetworkPlaneInternalEvent::IncomingDisconnected(self.sender.clone())).await {
                log::error!("Sending IncomingDisconnected error {:?}", err);
            }
        } else if let Err(err) = self.internal_tx.send(NetworkPlaneInternalEvent::OutgoingDisconnected(self.sender.clone())).await {
            log::error!("Sending OutgoingDisconnected error {:?}", err);
        }
    }
}

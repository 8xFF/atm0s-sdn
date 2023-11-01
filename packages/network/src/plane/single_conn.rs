use async_std::channel::Receiver;
use bluesea_router::{RouteAction, RouterTable};
use futures::{select, FutureExt, StreamExt};
use std::sync::Arc;
use utils::{option_handle::OptionUtils, Timer};

use crate::{
    behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction},
    transport::{ConnectionEvent, ConnectionReceiver, ConnectionSender},
};

use super::{bus::HandleEvent, bus::PlaneBus, bus_impl::PlaneBusImpl};

pub struct PlaneSingleConn<BE, HE> {
    pub(crate) sender: Arc<dyn ConnectionSender>,
    pub(crate) receiver: Box<dyn ConnectionReceiver + Send>,
    pub(crate) tick_ms: u64,
    pub(crate) tick_interval: async_std::stream::Interval,
    pub(crate) timer: Arc<dyn Timer>,
    pub(crate) bus_rx: Receiver<(u8, HandleEvent<HE>)>,
    pub(crate) router: Arc<dyn RouterTable>,
    pub(crate) bus: Arc<PlaneBusImpl<BE, HE>>,
    pub(crate) internal: PlaneSingleConnInternal<BE, HE>,
}

impl<BE, HE> PlaneSingleConn<BE, HE>
where
    BE: Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    pub fn start(&mut self) {
        self.internal.on_open(self.timer.now_ms());
        self.pop_actions();
    }

    pub async fn recv(&mut self) -> Result<(), ()> {
        let res = select! {
            _ = self.tick_interval.next().fuse() => {
                self.internal.on_tick(self.timer.now_ms(), self.tick_ms);
                Ok(())
            }
            e = self.bus_rx.recv().fuse() => {
                match e {
                    Ok((service_id, event)) => {
                        self.internal.on_bus_event(self.timer.now_ms(), service_id, event);
                        Ok(())
                    }
                    Err(_) => {
                        Err(())
                    }
                }
            }
            e = self.receiver.poll().fuse() => match e {
                Ok(event) => match event {
                    ConnectionEvent::Msg(msg) => match self.router.action_for_incomming(&msg.header.route, msg.header.service_id) {
                        RouteAction::Reject => {
                            Ok(())
                        }
                        RouteAction::Local => {
                            log::trace!(
                                "[NetworkPlane] fire handlers on_event network msg for conn ({}, {}) from service {}",
                                self.receiver.remote_node_id(),
                                self.receiver.conn_id(),
                                msg.header.service_id
                            );
                            self.internal.on_event(self.timer.now_ms(), Some(msg.header.service_id), ConnectionEvent::Msg(msg.clone()));
                            Ok(())
                        }
                        RouteAction::Next(conn, node_id) => {
                            log::trace!(
                                "[NetworkPlane] forward network msg {:?} for conn ({}, {}) to ({}, {}) from service {}, route {:?}",
                                msg,
                                self.receiver.remote_node_id(),
                                self.receiver.conn_id(),
                                conn,
                                node_id,
                                msg.header.service_id,
                                msg.header.route,
                            );
                            self.bus.to_net_conn(conn, msg.clone()).print_none("Should send to conn");
                            Ok(())
                        }
                    },
                    ConnectionEvent::Stats(stats) => {
                        log::debug!("[NetworkPlane] fire handlers on_event network stats for conn ({}, {})", self.receiver.remote_node_id(), self.receiver.conn_id());
                        self.internal.on_event(self.timer.now_ms(), None, ConnectionEvent::Stats(stats.clone()));
                        Ok(())
                    }
                },
                Err(_err) => {
                    Err(())
                }
            }
        };

        self.pop_actions();
        res
    }

    pub fn end(&mut self) {
        self.internal.on_close(self.timer.now_ms());
        self.pop_actions();
    }

    fn pop_actions(&mut self) {
        while let Some((service_id, action)) = self.internal.pop_action() {
            match action {
                ConnectionHandlerAction::ToBehaviour(event) => {
                    self.bus.to_behaviour(service_id, event);
                }
                ConnectionHandlerAction::ToNet(msg) => {
                    self.sender.send(msg);
                }
                ConnectionHandlerAction::ToNetConn(conn, msg) => {
                    self.bus.to_net_conn(conn, msg);
                }
                ConnectionHandlerAction::ToNetNode(node, msg) => {
                    self.bus.to_net_node(node, msg);
                }
                ConnectionHandlerAction::ToHandler(route, event) => {
                    self.bus
                        .to_handler(service_id, route, HandleEvent::FromHandler(self.receiver.remote_node_id(), self.receiver.conn_id(), event));
                }
                ConnectionHandlerAction::CloseConn() => {
                    self.sender.close();
                }
            }
        }
    }
}

pub(crate) struct PlaneSingleConnInternal<BE, HE> {
    pub(crate) handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, HE>>, ConnectionContext)>>,
}

impl<BE, HE> PlaneSingleConnInternal<BE, HE> {
    pub fn on_open(&mut self, now_ms: u64) {
        for (handler, context) in self.handlers.iter_mut().flatten() {
            handler.on_opened(context, now_ms);
        }
    }

    pub fn on_tick(&mut self, now_ms: u64, interval_ms: u64) {
        for (handler, context) in self.handlers.iter_mut().flatten() {
            handler.on_tick(context, now_ms, interval_ms);
        }
    }

    pub fn on_event(&mut self, now_ms: u64, service_id: Option<u8>, event: ConnectionEvent) {
        if let Some(service_id) = service_id {
            if let Some((handler, ctx)) = self.handlers[service_id as usize].as_mut() {
                handler.on_event(ctx, now_ms, event);
            } else {
                log::warn!("[NetworkPlane] service {} not found", service_id);
            }
        } else {
            for (handler, context) in self.handlers.iter_mut().flatten() {
                handler.on_event(context, now_ms, event.clone());
            }
        }
    }

    pub fn on_bus_event(&mut self, now_ms: u64, service_id: u8, event: HandleEvent<HE>) {
        if let Some((handler, context)) = self.handlers[service_id as usize].as_mut() {
            match event {
                HandleEvent::Awake => {
                    handler.on_awake(context, now_ms);
                }
                HandleEvent::FromBehavior(e) => {
                    handler.on_behavior_event(context, now_ms, e);
                }
                HandleEvent::FromHandler(node, conn, e) => {
                    handler.on_other_handler_event(context, now_ms, node, conn, e);
                }
            }
        } else {
            log::warn!("[NetworkPlane] service {} not found", service_id);
        }
    }

    pub fn on_close(&mut self, now_ms: u64) {
        for (handler, context) in self.handlers.iter_mut().flatten() {
            handler.on_closed(context, now_ms);
        }
    }

    pub fn pop_action(&mut self) -> Option<(u8, ConnectionHandlerAction<BE, HE>)> {
        for (handler, context) in self.handlers.iter_mut().flatten() {
            if let Some(action) = handler.pop_action() {
                return Some((context.service_id, action));
            }
        }
        None
    }
}

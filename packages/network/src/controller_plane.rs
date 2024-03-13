use std::{collections::HashMap, net::SocketAddr};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::core::{DestDelta, RegistryDelta, RegistryDestDelta, RouterDelta, TableDelta};
use atm0s_sdn_router::shadow::ShadowRouterDelta;

use crate::event::DataEvent;

use self::connections::{ConnectionCtx, ConnectionEvent, Connections};
use self::router_sync::RouterSyncLogic;

pub use self::connections::ConnectionMsg;
pub use self::feature::FeatureMsg;
pub use self::service::{Service, ServiceOutput};

mod connections;
pub mod feature;
mod router_sync;
pub mod service;

pub enum Input {
    ConnectTo(NodeAddr),
    Data(SocketAddr, DataEvent),
    ShutdownRequest,
}

pub enum Output {
    RouterRule(ShadowRouterDelta<SocketAddr>),
    Data(SocketAddr, DataEvent),
    ShutdownSuccess,
}

pub struct ControllerPlane {
    conns: Connections,
    router_sync: RouterSyncLogic,
    services: [Option<Box<dyn Service>>; 256],
    conns_addr: HashMap<SocketAddr, ConnectionCtx>,
    conns_id: HashMap<ConnId, ConnectionCtx>,
}

impl ControllerPlane {
    pub fn new(node_id: NodeId, mut services: Vec<Box<dyn Service>>) -> Self {
        #[cfg(feature = "vpn")]
        services.push(Box::new(self::service::vpn::VpnService));

        let mut services_id = vec![];
        for service in services.iter() {
            if service.discoverable() {
                services_id.push(service.service_type());
            }
        }

        // create service map
        let mut services_map: [Option<Box<dyn Service>>; 256] = std::array::from_fn(|_| None);
        for service in services {
            let service_type = service.service_type() as usize;
            if services_map[service_type].is_some() {
                panic!("Service type {} already exists", service_type);
            }
            log::info!("Add service {} {}", service.service_type(), service.service_name());
            services_map[service_type] = Some(service);
        }

        Self {
            conns: Connections::new(node_id),
            router_sync: RouterSyncLogic::new(node_id, services_id),
            services: services_map,
            conns_addr: HashMap::new(),
            conns_id: HashMap::new(),
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.conns.on_tick(now_ms);
        self.router_sync.on_tick(now_ms);
        for service in self.services.iter_mut().flatten() {
            service.on_tick(now_ms);
        }
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input) {
        match event {
            Input::ConnectTo(addr) => {
                self.conns.on_event(now_ms, connections::Input::ConnectTo(addr));
            }
            Input::Data(remote, DataEvent::Connection(msg)) => {
                self.conns.on_event(now_ms, connections::Input::NetIn(remote, msg));
            }
            Input::Data(remote, DataEvent::RouterSync(msg)) => {
                if let Some(ctx) = self.conns_addr.get(&remote) {
                    self.router_sync.on_conn_router_sync(now_ms, &ctx, msg);
                }
            }
            Input::Data(_remote, DataEvent::Feature(_msg)) => {
                todo!()
            }
            Input::Data(remote, DataEvent::Network(msg)) => {
                if let Some(ctx) = self.conns_addr.get(&remote) {
                    if let Some(service) = &mut self.services[msg.header.service as usize] {
                        service.on_conn_data(now_ms, ctx, msg.clone());
                    }
                }
            }
            Input::ShutdownRequest => {
                self.conns.on_event(now_ms, connections::Input::ShutdownRequest);
            }
        }
    }

    pub fn pop_output(&mut self, now_ms: u64) -> Option<Output> {
        if let Some(out) = self.conns.pop_output() {
            if let Some(out) = self.convert_conns_out(now_ms, out) {
                return Some(out);
            }
        }

        if let Some(out) = self.router_sync.pop_output() {
            if let Some(out) = self.convert_router_sync_out(now_ms, out) {
                return Some(out);
            }
        }

        for service in self.services.iter_mut().flatten() {
            if let Some(out) = service.pop_output() {
                return self.convert_service_out(now_ms, out);
            }
        }

        None
    }

    fn convert_conns_out(&mut self, now_ms: u64, out: connections::Output) -> Option<Output> {
        match out {
            connections::Output::NetOut(remote, msg) => Some(Output::Data(remote, DataEvent::Connection(msg))),
            connections::Output::ShutdownSuccess => Some(Output::ShutdownSuccess),
            connections::Output::ConnectionEvent(event) => {
                match event {
                    ConnectionEvent::ConnectionEstablished(ctx) => {
                        for service in self.services.iter_mut().flatten() {
                            service.on_conn_connected(now_ms, &ctx);
                        }
                        self.router_sync.on_conn_connected(now_ms, &ctx);
                        self.conns_addr.insert(ctx.remote, ctx.clone());
                        self.conns_id.insert(ctx.id, ctx);
                    }
                    ConnectionEvent::ConnectionStats(ctx, stats) => {
                        for service in self.services.iter_mut().flatten() {
                            service.on_conn_stats(now_ms, &ctx, &stats);
                        }
                        self.router_sync.on_conn_stats(now_ms, &ctx, &stats);
                    }
                    ConnectionEvent::ConnectionDisconnected(ctx) => {
                        for service in self.services.iter_mut().flatten() {
                            service.on_conn_disconnected(now_ms, &ctx);
                        }
                        self.router_sync.on_conn_disconnected(now_ms, &ctx);
                        self.conns_addr.remove(&ctx.remote);
                        self.conns_id.remove(&ctx.id);
                    }
                }
                None
            }
        }
    }

    fn convert_router_sync_out(&mut self, _now_ms: u64, out: router_sync::Output) -> Option<Output> {
        match out {
            router_sync::Output::NetData(id, msg) => {
                let conn = self.conns_id.get(&id)?;
                Some(Output::Data(conn.remote, DataEvent::RouterSync(msg)))
            }
            router_sync::Output::RouterRule(rule) => {
                let rule = match rule {
                    RouterDelta::Table(layer, TableDelta(index, DestDelta::SetBestPath(conn))) => ShadowRouterDelta::SetTable {
                        layer,
                        index,
                        next: self.conns_id.get(&conn)?.remote,
                    },
                    RouterDelta::Table(layer, TableDelta(index, DestDelta::DelBestPath)) => ShadowRouterDelta::DelTable { layer, index },
                    RouterDelta::Registry(RegistryDelta::SetServiceLocal(service)) => ShadowRouterDelta::SetServiceLocal { service },
                    RouterDelta::Registry(RegistryDelta::DelServiceLocal(service)) => ShadowRouterDelta::DelServiceLocal { service },
                    RouterDelta::Registry(RegistryDelta::ServiceRemote(service, RegistryDestDelta::SetServicePath(conn, dest, score))) => ShadowRouterDelta::SetServiceRemote {
                        service,
                        next: self.conns_id.get(&conn)?.remote,
                        dest,
                        score,
                    },
                    RouterDelta::Registry(RegistryDelta::ServiceRemote(service, RegistryDestDelta::DelServicePath(conn))) => ShadowRouterDelta::DelServiceRemote {
                        service,
                        next: self.conns_id.get(&conn)?.remote,
                    },
                };
                Some(Output::RouterRule(rule))
            }
        }
    }

    fn convert_service_out(&mut self, _now_ms: u64, out: ServiceOutput) -> Option<Output> {
        match out {
            ServiceOutput::NetData(id, msg) => {
                let conn = self.conns_id.get(&id)?;
                Some(Output::Data(conn.remote, DataEvent::Network(msg)))
            }
        }
    }
}

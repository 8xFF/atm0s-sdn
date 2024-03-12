use std::{collections::HashMap, net::SocketAddr};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_router::core::{DestDelta, RegistryDelta, RouterDelta, TableDelta};
use atm0s_sdn_router::shadow::ShadowRouterDelta;

use crate::event::DataEvent;

pub use self::connections::ControlMsg;
pub use self::service::{Service, ServiceOutput};

use self::connections::{ConnectionCtx, ConnectionEvent, Connections};

mod connections;
pub mod core_services;
mod service;

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
    services: [Option<Box<dyn Service>>; 256],
    conns_addr: HashMap<SocketAddr, ConnectionCtx>,
    conns_id: HashMap<ConnId, ConnectionCtx>,
}

impl ControllerPlane {
    pub fn new(node_id: NodeId, mut services: Vec<Box<dyn Service>>) -> Self {
        let mut services_id = vec![
            core_services::router_sync::SERVICE_TYPE,
            #[cfg(feature = "vpn")]
            core_services::vpn::SERVICE_TYPE,
        ];
        for service in services.iter() {
            services_id.push(service.service_type());
        }

        // add core services
        services.push(Box::new(core_services::router_sync::RouterSyncService::new(node_id, services_id)));
        #[cfg(feature = "vpn")]
        services.push(Box::new(core_services::vpn::VpnService::default()));

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
            services: services_map,
            conns_addr: HashMap::new(),
            conns_id: HashMap::new(),
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.conns.on_tick(now_ms);
        for service in self.services.iter_mut().flatten() {
            service.on_tick(now_ms);
        }
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input) {
        match event {
            Input::ConnectTo(addr) => {
                self.conns.on_event(now_ms, connections::Input::ConnectTo(addr));
            }
            Input::Data(remote, DataEvent::Control(msg)) => {
                self.conns.on_event(now_ms, connections::Input::ControlIn(remote, msg));
            }
            Input::Data(remote, DataEvent::Network(msg)) => {
                if let Some(ctx) = self.conns_addr.get(&remote) {
                    if let Some(service) = &mut self.services[msg.header.to_service_id as usize] {
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
        if let Some(res) = self.conns.pop_output() {
            return self.convert_conns_out(now_ms, res);
        } else {
            for service in self.services.iter_mut().flatten() {
                if let Some(out) = service.pop_output() {
                    return self.convert_service_out(now_ms, out);
                }
            }

            None
        }
    }

    fn convert_conns_out(&mut self, now_ms: u64, out: connections::Output) -> Option<Output> {
        match out {
            connections::Output::ControlOut(remote, msg) => Some(Output::Data(remote, DataEvent::Control(msg))),
            connections::Output::ShutdownSuccess => Some(Output::ShutdownSuccess),
            connections::Output::ConnectionEvent(event) => {
                match event {
                    ConnectionEvent::ConnectionEstablished(ctx) => {
                        for service in self.services.iter_mut().flatten() {
                            service.on_conn_connected(now_ms, &ctx);
                        }
                        self.conns_addr.insert(ctx.remote, ctx.clone());
                        self.conns_id.insert(ctx.id, ctx);
                    }
                    ConnectionEvent::ConnectionStats(ctx, stats) => {
                        for service in self.services.iter_mut().flatten() {
                            service.on_conn_stats(now_ms, &ctx, &stats);
                        }
                    }
                    ConnectionEvent::ConnectionDisconnected(ctx) => {
                        for service in self.services.iter_mut().flatten() {
                            service.on_conn_disconnected(now_ms, &ctx);
                        }
                        self.conns_addr.remove(&ctx.remote);
                        self.conns_id.remove(&ctx.id);
                    }
                }
                None
            }
        }
    }

    fn convert_service_out(&mut self, _now_ms: u64, out: ServiceOutput) -> Option<Output> {
        match out {
            ServiceOutput::RouterRule(rule) => {
                let rule = match rule {
                    RouterDelta::Table(layer, TableDelta(index, DestDelta::SetBestPath(conn))) => ShadowRouterDelta::SetTable {
                        layer,
                        index,
                        remote: self.conns_id.get(&conn)?.remote,
                    },
                    RouterDelta::Table(layer, TableDelta(index, DestDelta::DelBestPath)) => ShadowRouterDelta::DelTable { layer, index },
                    RouterDelta::Registry(RegistryDelta::SetServiceLocal(service)) => ShadowRouterDelta::SetServiceLocal { service },
                    RouterDelta::Registry(RegistryDelta::DelServiceLocal(service)) => ShadowRouterDelta::DelServiceLocal { service },
                    RouterDelta::Registry(RegistryDelta::ServiceRemote(service, DestDelta::SetBestPath(conn))) => ShadowRouterDelta::SetServiceRemote {
                        service,
                        remote: self.conns_id.get(&conn)?.remote,
                    },
                    RouterDelta::Registry(RegistryDelta::ServiceRemote(service, DestDelta::DelBestPath)) => ShadowRouterDelta::DelServiceRemote { service },
                };
                Some(Output::RouterRule(rule))
            }
            ServiceOutput::NetData(id, msg) => {
                let conn = self.conns_id.get(&id)?;
                Some(Output::Data(conn.remote, DataEvent::Network(msg)))
            }
        }
    }
}

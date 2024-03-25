use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{
    core::{DestDelta, Metric, RegistryDelta, RegistryDestDelta, Router, RouterDelta, RouterSync, TableDelta},
    shadow::ShadowRouterDelta,
};

use crate::base::{
    ConnectionEvent, Feature, FeatureContext, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, NetOutgoingMeta,
};

pub const FEATURE_ID: u8 = 2;
pub const FEATURE_NAME: &str = "router_sync";

const INIT_RTT_MS: u16 = 1000;
const INIT_BW: u32 = 100_000_000;

pub type Control = ();
pub type Event = ();

pub type ToWorker = ShadowRouterDelta<SocketAddr>;
pub type ToController = ();

pub struct RouterSyncFeature {
    router: Router,
    conns: HashMap<ConnId, (NodeId, SocketAddr, Metric)>,
    queue: VecDeque<FeatureOutput<Event, ToWorker>>,
    services: Vec<u8>,
}

impl RouterSyncFeature {
    pub fn new(node: NodeId, services: Vec<u8>) -> Self {
        log::info!("[RouterSync] started node {} with public services {:?}", node, services);

        Self {
            router: Router::new(node),
            services,
            conns: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    fn send_sync_to(router: &Router, queue: &mut VecDeque<FeatureOutput<Event, ToWorker>>, conn: ConnId, node: NodeId) {
        let sync = router.create_sync(node);
        queue.push_back(FeatureOutput::SendDirect(conn, NetOutgoingMeta::default(), bincode::serialize(&sync).expect("").into()));
    }
}

impl Feature<Control, Event, ToController, ToWorker> for RouterSyncFeature {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Tick(tick_count) => {
                if tick_count < 1 {
                    //we need to wait all workers to be ready
                    return;
                }

                while let Some(service) = self.services.pop() {
                    log::info!("[RouterSync] register local service {}", service);
                    self.router.register_service(service);
                }

                for (conn, (node, _, _)) in self.conns.iter() {
                    Self::send_sync_to(&self.router, &mut self.queue, *conn, *node);
                }
            }
            FeatureSharedInput::Connection(event) => match event {
                ConnectionEvent::Connected(ctx, _) => {
                    log::info!("[RouterSync] Connection {} connected", ctx.remote);
                    let metric = Metric::new(INIT_RTT_MS, vec![ctx.node], INIT_BW);
                    self.conns.insert(ctx.conn, (ctx.node, ctx.remote, metric.clone()));
                    self.router.set_direct(ctx.conn, metric);
                    Self::send_sync_to(&self.router, &mut self.queue, ctx.conn, ctx.node);
                }
                ConnectionEvent::Stats(ctx, stats) => {
                    log::debug!("[RouterSync] Connection {} stats rtt_ms {}", ctx.remote, stats.rtt_ms);
                    let metric = Metric::new(stats.rtt_ms as u16, vec![ctx.node], INIT_BW);
                    self.conns.insert(ctx.conn, (ctx.node, ctx.remote, metric.clone()));
                    self.router.set_direct(ctx.conn, metric);
                }
                ConnectionEvent::Disconnected(ctx) => {
                    log::info!("[RouterSync] Connection {} disconnected", ctx.remote);
                    self.conns.remove(&ctx.conn);
                    self.router.del_direct(ctx.conn);
                }
            },
        }
    }

    fn on_input<'a>(&mut self, _ctx: &FeatureContext, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Net(ctx, _header, buf) => {
                if let Some((_node, _remote, metric)) = self.conns.get(&ctx.conn) {
                    if let Ok(sync) = bincode::deserialize::<RouterSync>(&buf) {
                        self.router.apply_sync(ctx.conn, metric.clone(), sync);
                    } else {
                        log::warn!("[RouterSync] Receive invalid sync from {}", ctx.remote);
                    }
                } else {
                    log::warn!("[RouterSync] Receive sync from unknown connection {}", ctx.remote);
                }
            }
            _ => {}
        }
    }

    fn pop_output<'a>(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<Event, ToWorker>> {
        if let Some(rule) = self.router.pop_delta() {
            log::debug!("[RouterSync] broadcast to all workers {:?}", rule);
            let rule = match rule {
                RouterDelta::Table(layer, TableDelta(index, DestDelta::SetBestPath(conn))) => ShadowRouterDelta::SetTable {
                    layer,
                    index,
                    next: self.conns.get(&conn)?.1,
                },
                RouterDelta::Table(layer, TableDelta(index, DestDelta::DelBestPath)) => ShadowRouterDelta::DelTable { layer, index },
                RouterDelta::Registry(RegistryDelta::SetServiceLocal(service)) => ShadowRouterDelta::SetServiceLocal { service },
                RouterDelta::Registry(RegistryDelta::DelServiceLocal(service)) => ShadowRouterDelta::DelServiceLocal { service },
                RouterDelta::Registry(RegistryDelta::ServiceRemote(service, RegistryDestDelta::SetServicePath(conn, dest, score))) => {
                    let conn = self.conns.get(&conn)?;
                    ShadowRouterDelta::SetServiceRemote {
                        service,
                        conn: conn.1,
                        next: conn.0,
                        dest,
                        score,
                    }
                }
                RouterDelta::Registry(RegistryDelta::ServiceRemote(service, RegistryDestDelta::DelServicePath(conn))) => ShadowRouterDelta::DelServiceRemote {
                    service,
                    conn: self.conns.get(&conn)?.1,
                },
            };
            return Some(FeatureOutput::ToWorker(true, rule));
        }
        self.queue.pop_front()
    }
}

#[derive(Default)]
pub struct RouterSyncFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for RouterSyncFeatureWorker {
    fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<'a, Control, ToWorker>) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        match input {
            FeatureWorkerInput::Control(service, control) => Some(FeatureWorkerOutput::ForwardControlToController(service, control)),
            FeatureWorkerInput::Network(conn, header, msg) => Some(FeatureWorkerOutput::ForwardNetworkToController(conn, header, msg.to_vec())),
            FeatureWorkerInput::FromController(_, delta) => {
                log::debug!("[RouterSyncWorker] apply router delta {:?}", delta);
                ctx.router.apply_delta(delta);
                None
            }
            FeatureWorkerInput::Local(_header, _msg) => {
                log::warn!("No handler for local message in {}", FEATURE_NAME);
                None
            }
            FeatureWorkerInput::TunPkt(_buf) => {
                log::warn!("No handler for tun packet in {}", FEATURE_NAME);
                None
            }
        }
    }
}

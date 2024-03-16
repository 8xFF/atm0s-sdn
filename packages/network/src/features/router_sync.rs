use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{
    core::{DestDelta, Metric, RegistryDelta, RegistryDestDelta, Router, RouterDelta, RouterSync, TableDelta},
    shadow::ShadowRouterDelta,
};

use crate::base::{ConnectionEvent, Feature, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput};

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
}

impl RouterSyncFeature {
    pub fn new(node: NodeId) -> Self {
        let mut router = Router::new(node);
        router.register_service(100);

        Self {
            router,
            conns: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    fn send_sync_to(router: &Router, queue: &mut VecDeque<FeatureOutput<Event, ToWorker>>, conn: ConnId, node: NodeId) {
        let sync = router.create_sync(node);
        queue.push_back(FeatureOutput::SendDirect(conn, bincode::serialize(&sync).expect("").into()));
    }
}

impl Feature<Control, Event, ToController, ToWorker> for RouterSyncFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, _now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Tick(_) => {
                for (conn, (node, _, _)) in self.conns.iter() {
                    Self::send_sync_to(&self.router, &mut self.queue, *conn, *node);
                }
            }
            FeatureSharedInput::Connection(event) => match event {
                ConnectionEvent::Connected(ctx, _) => {
                    log::info!("Connection {} connected", ctx.remote);
                    let metric = Metric::new(INIT_RTT_MS, vec![ctx.node], INIT_BW);
                    self.conns.insert(ctx.conn, (ctx.node, ctx.remote, metric.clone()));
                    self.router.set_direct(ctx.conn, metric);
                    Self::send_sync_to(&self.router, &mut self.queue, ctx.conn, ctx.node);
                }
                ConnectionEvent::Stats(ctx, stats) => {
                    log::debug!("Connection {} stats rtt_ms {}", ctx.remote, stats.rtt_ms);
                    self.router.set_direct(ctx.conn, Metric::new(stats.rtt_ms as u16, vec![ctx.node], INIT_BW));
                }
                ConnectionEvent::Disconnected(ctx) => {
                    log::info!("Connection {} disconnected", ctx.remote);
                    self.conns.remove(&ctx.conn);
                    self.router.del_direct(ctx.conn);
                }
            },
        }
    }

    fn on_input<'a>(&mut self, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::ForwardNetFromWorker(ctx, buf) => {
                if let Some((_node, _remote, metric)) = self.conns.get(&ctx.conn) {
                    if let Ok(sync) = bincode::deserialize::<RouterSync>(&buf) {
                        self.router.apply_sync(ctx.conn, metric.clone(), sync);
                    } else {
                        log::warn!("Receive invalid sync from {}", ctx.remote);
                    }
                } else {
                    log::warn!("Receive sync from unknown connection {}", ctx.remote);
                }
            }
            _ => {}
        }
    }

    fn pop_output<'a>(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        if let Some(rule) = self.router.pop_delta() {
            let rule = match rule {
                RouterDelta::Table(layer, TableDelta(index, DestDelta::SetBestPath(conn))) => ShadowRouterDelta::SetTable {
                    layer,
                    index,
                    next: self.conns.get(&conn)?.1,
                },
                RouterDelta::Table(layer, TableDelta(index, DestDelta::DelBestPath)) => ShadowRouterDelta::DelTable { layer, index },
                RouterDelta::Registry(RegistryDelta::SetServiceLocal(service)) => ShadowRouterDelta::SetServiceLocal { service },
                RouterDelta::Registry(RegistryDelta::DelServiceLocal(service)) => ShadowRouterDelta::DelServiceLocal { service },
                RouterDelta::Registry(RegistryDelta::ServiceRemote(service, RegistryDestDelta::SetServicePath(conn, dest, score))) => ShadowRouterDelta::SetServiceRemote {
                    service,
                    next: self.conns.get(&conn)?.1,
                    dest,
                    score,
                },
                RouterDelta::Registry(RegistryDelta::ServiceRemote(service, RegistryDestDelta::DelServicePath(conn))) => ShadowRouterDelta::DelServiceRemote {
                    service,
                    next: self.conns.get(&conn)?.1,
                },
            };
            return Some(FeatureOutput::BroadcastToWorkers(rule));
        }
        self.queue.pop_front()
    }
}

#[derive(Default)]
pub struct RouterSyncFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for RouterSyncFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<'a, Control, ToWorker>) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        match input {
            FeatureWorkerInput::Control(service, control) => Some(FeatureWorkerOutput::ForwardControlToController(service, control)),
            FeatureWorkerInput::Network(conn, msg) => Some(FeatureWorkerOutput::ForwardNetworkToController(conn, msg.to_vec())),
            FeatureWorkerInput::FromController(delta) => {
                ctx.router.apply_delta(delta);
                None
            }
            FeatureWorkerInput::Local(_msg) => {
                log::warn!("No handler for local message in {}", self.feature_name());
                None
            }
        }
    }
}
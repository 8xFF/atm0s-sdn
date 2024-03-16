use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{
    core::{DestDelta, Metric, RegistryDelta, RegistryDestDelta, Router, RouterDelta, RouterSync, TableDelta},
    shadow::ShadowRouterDelta,
    RouteRule,
};

use crate::base::{
    ConnectionEvent, Feature, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, TransportMsg, TransportMsgHeader,
};

pub const FEATURE_ID: u8 = 2;
pub const FEATURE_NAME: &str = "router_sync";

const INIT_RTT_MS: u16 = 1000;
const INIT_BW: u32 = 100_000_000;

#[derive(Debug, Clone)]
pub enum Control {
    Send(RouteRule, Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum Event {
    Data(Vec<u8>),
}

pub type ToWorker = ShadowRouterDelta<SocketAddr>;

#[derive(Debug, Clone)]
pub struct ToController;

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
        let header = TransportMsgHeader::build(FEATURE_ID, 0, RouteRule::Direct);
        let msg = TransportMsg::from_payload_bincode(header, &sync);
        queue.push_back(FeatureOutput::SendDirect(conn, msg));
    }
}

impl Feature<Control, Event, ToController, ToWorker> for RouterSyncFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_input<'a>(&mut self, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Shared(FeatureSharedInput::Tick(_)) => {
                for (conn, (node, _, _)) in self.conns.iter() {
                    Self::send_sync_to(&self.router, &mut self.queue, *conn, *node);
                }
            }
            FeatureInput::Shared(FeatureSharedInput::Connection(event)) => match event {
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
            FeatureInput::ForwardNetFromWorker(ctx, msg) => {
                if let Some((_node, _remote, metric)) = self.conns.get(&ctx.conn) {
                    if let Ok(sync) = msg.get_payload_bincode::<RouterSync>() {
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

    fn pop_output(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
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

    fn on_input(&mut self, ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<Control, ToWorker>) -> Option<FeatureWorkerOutput<Control, Event, ToController>> {
        match input {
            FeatureWorkerInput::Control(service, control) => Some(FeatureWorkerOutput::ForwardControlToController(service, control)),
            FeatureWorkerInput::Network(conn, msg) => Some(FeatureWorkerOutput::ForwardNetworkToController(conn, msg)),
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

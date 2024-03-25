use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{
    shadow::{ShadowRouter, ShadowRouterHistory},
    RouteAction, RouteRule, RouterTable,
};

use crate::{
    base::{
        FeatureControlActor, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer, GenericBufferMut, NeighboursControl, NetOutgoingMeta, ServiceBuilder, ServiceControlActor,
        ServiceId, ServiceWorkerCtx, ServiceWorkerInput, ServiceWorkerOutput, TransportMsg, TransportMsgHeader, Ttl,
    },
    features::{Features, FeaturesControl, FeaturesEvent, FeaturesToController},
    san_io_utils::TasksSwitcher,
    ExtOut, LogicControl, LogicEvent,
};

use self::{connection::DataPlaneConnection, features::FeatureWorkerManager, services::ServiceWorkerManager};

mod connection;
mod features;
mod services;

#[derive(Debug)]
pub enum NetInput<'a> {
    UdpPacket(SocketAddr, GenericBufferMut<'a>),
    TunPacket(GenericBufferMut<'a>),
}

#[derive(Debug)]
pub enum Input<'a, TW> {
    Net(NetInput<'a>),
    Event(LogicEvent<TW>),
    ShutdownRequest,
}

#[derive(Debug)]
pub enum NetOutput<'a> {
    UdpPacket(SocketAddr, GenericBuffer<'a>),
    UdpPackets(Vec<SocketAddr>, GenericBuffer<'a>),
    TunPacket(GenericBuffer<'a>),
}

#[derive(convert_enum::From)]
pub enum Output<'a, SE, TC> {
    Ext(ExtOut<SE>),
    Net(NetOutput<'a>),
    Control(LogicControl<TC>),
    #[convert_enum(optout)]
    ShutdownResponse,
    #[convert_enum(optout)]
    Continue,
}

#[repr(u8)]
enum TaskType {
    Feature,
    Service,
}

enum QueueOutput<SE, TC> {
    Feature(Features, FeatureWorkerOutput<'static, FeaturesControl, FeaturesEvent, FeaturesToController>),
    Service(ServiceId, ServiceWorkerOutput<FeaturesControl, FeaturesEvent, SE, TC>),
}

pub struct DataPlane<SC, SE, TC, TW> {
    tick_count: u64,
    feature_ctx: FeatureWorkerContext,
    features: FeatureWorkerManager,
    service_ctx: ServiceWorkerCtx,
    services: ServiceWorkerManager<SC, SE, TC, TW>,
    conns: HashMap<SocketAddr, DataPlaneConnection>,
    conns_reverse: HashMap<ConnId, SocketAddr>,
    queue_output: VecDeque<QueueOutput<SE, TC>>,
    switcher: TasksSwitcher<u8, 2>,
}

impl<SC, SE, TC, TW> DataPlane<SC, SE, TC, TW> {
    pub fn new(node_id: NodeId, services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>, history: Arc<dyn ShadowRouterHistory>) -> Self {
        log::info!("Create DataPlane for node: {}", node_id);

        Self {
            tick_count: 0,
            feature_ctx: FeatureWorkerContext {
                node_id,
                router: ShadowRouter::new(node_id, history),
            },
            features: FeatureWorkerManager::new(),
            service_ctx: ServiceWorkerCtx { node_id },
            services: ServiceWorkerManager::new(services),
            conns: HashMap::new(),
            conns_reverse: HashMap::new(),
            queue_output: VecDeque::new(),
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn route(&self, rule: RouteRule, source: Option<NodeId>, relay_from: Option<NodeId>) -> RouteAction<SocketAddr> {
        self.feature_ctx.router.derive_action(&rule, source, relay_from)
    }

    pub fn on_tick<'a>(&mut self, now_ms: u64) {
        self.switcher.push_all();
        self.features.on_tick(&mut self.feature_ctx, now_ms, self.tick_count);
        self.services.on_tick(&mut self.service_ctx, now_ms, self.tick_count);
        self.tick_count += 1;
    }

    pub fn on_event<'a>(&mut self, now_ms: u64, event: Input<'a, TW>) -> Option<Output<'a, SE, TC>> {
        match event {
            Input::Net(NetInput::UdpPacket(remote, buf)) => {
                if let Ok(control) = NeighboursControl::try_from(&*buf) {
                    Some(LogicControl::NetNeighbour(remote, control).into())
                } else {
                    self.incoming_route(now_ms, remote, buf)
                }
            }
            Input::Net(NetInput::TunPacket(pkt)) => {
                let out = self.features.on_input(&mut self.feature_ctx, Features::Vpn, now_ms, FeatureWorkerInput::TunPkt(pkt))?;
                Some(self.convert_features(now_ms, Features::Vpn, out))
            }
            Input::Event(LogicEvent::Feature(is_broadcast, to)) => {
                let feature = to.to_feature();
                let out = self.features.on_input(&mut self.feature_ctx, feature, now_ms, FeatureWorkerInput::FromController(is_broadcast, to))?;
                Some(self.convert_features(now_ms, feature, out))
            }
            Input::Event(LogicEvent::Service(service, to)) => {
                let out = self.services.on_input(&mut self.service_ctx, now_ms, service, ServiceWorkerInput::FromController(to))?;
                Some(self.convert_services(now_ms, service, out))
            }
            Input::Event(LogicEvent::NetNeighbour(remote, control)) => {
                let buf = (&control).try_into().ok()?;
                Some(NetOutput::UdpPacket(remote, GenericBuffer::Vec(buf)).into())
            }
            Input::Event(LogicEvent::NetDirect(feature, conn, meta, buf)) => {
                let addr = self.conns_reverse.get(&conn)?;
                let header = meta.to_header(feature as u8, RouteRule::Direct, self.feature_ctx.node_id);
                let msg = TransportMsg::build_raw(header, &buf);
                Some(NetOutput::UdpPacket(*addr, msg.take().into()).into())
            }
            Input::Event(LogicEvent::NetRoute(feature, rule, meta, buf)) => self.outgoing_route(now_ms, feature, rule, meta, buf),
            Input::Event(LogicEvent::Pin(conn, node, addr, secure)) => {
                self.conns.insert(addr, DataPlaneConnection::new(node, conn, addr, secure));
                self.conns_reverse.insert(conn, addr);
                None
            }
            Input::Event(LogicEvent::UnPin(conn)) => {
                if let Some(addr) = self.conns_reverse.remove(&conn) {
                    log::info!("UnPin: conn: {} <--> addr: {}", conn, addr);
                    self.conns.remove(&addr);
                }
                None
            }
            Input::ShutdownRequest => Some(Output::ShutdownResponse),
        }
    }

    pub fn pop_output<'a>(&mut self, now_ms: u64) -> Option<Output<'a, SE, TC>> {
        if let Some(out) = self.queue_output.pop_front() {
            return match out {
                QueueOutput::Feature(feature, out) => Some(self.convert_features(now_ms, feature, out)),
                QueueOutput::Service(service, out) => Some(self.convert_services(now_ms, service, out)),
            };
        };

        while let Some(current) = self.switcher.current() {
            match current {
                0 => {
                    let out = self.features.pop_output(&mut self.feature_ctx);
                    if let Some((feature, out)) = self.switcher.process(out) {
                        return Some(self.convert_features(now_ms, feature, out));
                    }
                }
                1 => {
                    let out = self.services.pop_output(&mut self.service_ctx);
                    if let Some((feature, out)) = self.switcher.process(out) {
                        return Some(self.convert_services(now_ms, feature, out));
                    }
                }
                _ => panic!("Invalid task group index: {}", current),
            }
        }

        None
    }

    fn incoming_route<'a>(&mut self, now_ms: u64, remote: SocketAddr, mut buf: GenericBufferMut<'a>) -> Option<Output<'a, SE, TC>> {
        let conn = self.conns.get(&remote)?;
        let header = TransportMsgHeader::try_from(&buf as &[u8]).ok()?;
        let action = self.feature_ctx.router.derive_action(&header.route, header.from_node, Some(conn.node()));
        log::debug!("[DataPlame] Incoming rule: {:?} from: {remote}, node {:?} => action {:?}", header.route, header.from_node, action);
        match action {
            RouteAction::Reject => None,
            RouteAction::Local => {
                let feature = header.feature.try_into().ok()?;
                log::debug!("Incoming feature: {:?} from: {remote}", feature);
                let out = self.features.on_network_raw(&mut self.feature_ctx, feature, now_ms, conn.conn(), remote, header, buf.to_readonly())?;
                Some(self.convert_features(now_ms, feature, out))
            }
            RouteAction::Next(remote) => {
                if !TransportMsgHeader::decrease_ttl(&mut buf) {
                    log::debug!("TTL is 0, drop packet");
                }
                Some(NetOutput::UdpPacket(remote, buf.to_readonly()).into())
            }
            RouteAction::Broadcast(local, remotes) => {
                if !TransportMsgHeader::decrease_ttl(&mut buf) {
                    log::debug!("TTL is 0, drop packet");
                    return None;
                }
                let buf = buf.to_readonly();
                if local {
                    if let Ok(feature) = header.feature.try_into() {
                        log::debug!("Incoming broadcast feature: {:?} from: {remote}", feature);
                        if let Some(out) = self.features.on_network_raw(&mut self.feature_ctx, feature, now_ms, conn.conn(), remote, header, buf.clone()) {
                            self.queue_output.push_back(QueueOutput::Feature(feature, out.owned()));
                        }
                    }
                }
                Some(NetOutput::UdpPackets(remotes, buf).into())
            }
        }
    }

    fn outgoing_route<'a>(&mut self, now_ms: u64, feature: Features, rule: RouteRule, mut meta: NetOutgoingMeta, buf: Vec<u8>) -> Option<Output<'a, SE, TC>> {
        match self.feature_ctx.router.derive_action(&rule, Some(self.feature_ctx.node_id), None) {
            RouteAction::Reject => {
                log::debug!("[DataPlane] outgoing route rule {:?} is rejected", rule);
                None
            }
            RouteAction::Local => {
                log::debug!("[DataPlane] outgoing route rule {:?} is processed locally", rule);
                let meta = meta.to_incoming(self.feature_ctx.node_id);
                let out = self.features.on_input(&mut self.feature_ctx, feature, now_ms, FeatureWorkerInput::Local(meta, buf.into()))?;
                Some(self.convert_features(now_ms, feature, out.owned()))
            }
            RouteAction::Next(remote) => {
                log::debug!("[DataPlane] outgoing route rule {:?} is go with remote {remote}", rule);
                let header = meta.to_header(feature as u8, rule, self.feature_ctx.node_id);
                let msg = TransportMsg::build_raw(header, &buf);
                Some(NetOutput::UdpPacket(remote, msg.take().into()).into())
            }
            RouteAction::Broadcast(local, remotes) => {
                log::debug!("[DataPlane] outgoing route rule {:?} is go with local {local} and remotes {:?}", rule, remotes);
                meta.source = true; //Force enable source for broadcast

                let header = meta.to_header(feature as u8, rule, self.feature_ctx.node_id);
                let msg = TransportMsg::build_raw(header, &buf);
                if local {
                    let meta = meta.to_incoming(self.feature_ctx.node_id);
                    if let Some(out) = self.features.on_input(&mut self.feature_ctx, feature, now_ms, FeatureWorkerInput::Local(meta, buf.into())) {
                        self.switcher.push_last(TaskType::Feature as u8);
                        self.queue_output.push_back(QueueOutput::Feature(feature, out.owned()));
                    }
                }

                Some(NetOutput::UdpPackets(remotes, msg.take().into()).into())
            }
        }
    }

    fn convert_features<'a>(&mut self, now_ms: u64, feature: Features, out: FeatureWorkerOutput<'a, FeaturesControl, FeaturesEvent, FeaturesToController>) -> Output<'a, SE, TC> {
        self.switcher.push_last(TaskType::Feature as u8);

        match out {
            FeatureWorkerOutput::ForwardControlToController(service, control) => LogicControl::FeaturesControl(service, control).into(),
            FeatureWorkerOutput::ForwardNetworkToController(conn, header, msg) => LogicControl::NetRemote(feature, conn, header, msg).into(),
            FeatureWorkerOutput::ForwardLocalToController(header, buf) => LogicControl::NetLocal(feature, header, buf).into(),
            FeatureWorkerOutput::ToController(control) => LogicControl::Feature(control).into(),
            FeatureWorkerOutput::Event(actor, event) => match actor {
                FeatureControlActor::Controller => Output::Ext(ExtOut::FeaturesEvent(event)),
                FeatureControlActor::Service(service) => {
                    if let Some(out) = self.services.on_input(&mut self.service_ctx, now_ms, service, ServiceWorkerInput::FeatureEvent(event)) {
                        self.switcher.push_last(TaskType::Service as u8);
                        self.queue_output.push_back(QueueOutput::Service(service, out));
                    }
                    Output::Continue
                }
            },
            FeatureWorkerOutput::SendDirect(conn, meta, buf) => {
                if let Some(addr) = self.conns_reverse.get(&conn) {
                    let header = meta.to_header(feature as u8, RouteRule::Direct, self.feature_ctx.node_id);
                    let msg = TransportMsg::build_raw(header, &buf);
                    NetOutput::UdpPacket(*addr, msg.take().into()).into()
                } else {
                    Output::Continue
                }
            }
            FeatureWorkerOutput::SendRoute(rule, ttl, buf) => {
                log::info!("SendRoute: {:?}", rule);
                if let Some(out) = self.outgoing_route(now_ms, feature, rule, ttl, buf) {
                    out
                } else {
                    Output::Continue
                }
            }
            FeatureWorkerOutput::RawDirect(conn, buf) => {
                if let Some(addr) = self.conns_reverse.get(&conn) {
                    NetOutput::UdpPacket(*addr, buf).into()
                } else {
                    Output::Continue
                }
            }
            FeatureWorkerOutput::RawBroadcast(conns, buf) => {
                let addrs = conns.iter().filter_map(|conn| self.conns_reverse.get(conn)).cloned().collect();
                NetOutput::UdpPackets(addrs, buf).into()
            }
            FeatureWorkerOutput::RawDirect2(addr, buf) => NetOutput::UdpPacket(addr, buf).into(),
            FeatureWorkerOutput::RawBroadcast2(addrs, buf) => NetOutput::UdpPackets(addrs, buf).into(),
            FeatureWorkerOutput::TunPkt(pkt) => NetOutput::TunPacket(pkt).into(),
        }
    }

    fn convert_services<'a>(&mut self, now_ms: u64, service: ServiceId, out: ServiceWorkerOutput<FeaturesControl, FeaturesEvent, SE, TC>) -> Output<'a, SE, TC> {
        self.switcher.push_last(TaskType::Service as u8);

        match out {
            ServiceWorkerOutput::ForwardFeatureEventToController(event) => LogicControl::ServiceEvent(service, event).into(),
            ServiceWorkerOutput::ToController(tc) => LogicControl::Service(service, tc).into(),
            ServiceWorkerOutput::FeatureControl(control) => {
                let feature = control.to_feature();
                if let Some(out) = self
                    .features
                    .on_input(&mut self.feature_ctx, feature, now_ms, FeatureWorkerInput::Control(FeatureControlActor::Service(service), control))
                {
                    self.switcher.push_last(TaskType::Feature as u8);
                    self.queue_output.push_back(QueueOutput::Feature(feature, out));
                }
                Output::Continue
            }
            ServiceWorkerOutput::Event(actor, event) => match actor {
                ServiceControlActor::Controller => Output::Ext(ExtOut::ServicesEvent(event)),
            },
        }
    }
}

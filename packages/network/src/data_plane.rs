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
use sans_io_runtime::TaskSwitcher;

use crate::{
    base::{
        Buffer, BufferMut, FeatureControlActor, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, NeighboursControl, NetOutgoingMeta, ServiceBuilder, ServiceControlActor, ServiceId,
        ServiceWorkerCtx, ServiceWorkerInput, ServiceWorkerOutput, TransportMsg, TransportMsgHeader,
    },
    features::{Features, FeaturesControl, FeaturesEvent, FeaturesToController},
    ExtIn, ExtOut, LogicControl, LogicEvent,
};

use self::{connection::DataPlaneConnection, features::FeatureWorkerManager, services::ServiceWorkerManager};

mod connection;
mod features;
mod services;

#[derive(Debug)]
pub enum NetInput<'a> {
    UdpPacket(SocketAddr, BufferMut<'a>),
    TunPacket(BufferMut<'a>),
}

#[derive(Debug, Clone)]
pub enum CrossWorker<SE> {
    Feature(FeaturesEvent),
    Service(ServiceId, SE),
}

#[derive(Debug)]
pub enum Input<'a, SC, SE, TW> {
    Ext(ExtIn<SC>),
    Net(NetInput<'a>),
    Event(LogicEvent<SE, TW>),
    Worker(CrossWorker<SE>),
    ShutdownRequest,
}

#[derive(Debug)]
pub enum NetOutput<'a> {
    UdpPacket(SocketAddr, Buffer<'a>),
    UdpPackets(Vec<SocketAddr>, Buffer<'a>),
    TunPacket(Buffer<'a>),
}

#[derive(convert_enum::From)]
pub enum Output<'a, SC, SE, TC> {
    Ext(ExtOut<SE>),
    Net(NetOutput<'a>),
    Control(LogicControl<SC, SE, TC>),
    #[convert_enum(optout)]
    Worker(u16, CrossWorker<SE>),
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

enum QueueOutput<SC, SE, TC> {
    Feature(Features, FeatureWorkerOutput<'static, FeaturesControl, FeaturesEvent, FeaturesToController>),
    Service(ServiceId, ServiceWorkerOutput<FeaturesControl, FeaturesEvent, SC, SE, TC>),
    Net(NetOutput<'static>),
}

pub struct DataPlane<SC, SE, TC, TW> {
    tick_count: u64,
    worker_id: u16,
    feature_ctx: FeatureWorkerContext,
    features: FeatureWorkerManager,
    service_ctx: ServiceWorkerCtx,
    services: ServiceWorkerManager<SC, SE, TC, TW>,
    conns: HashMap<SocketAddr, DataPlaneConnection>,
    conns_reverse: HashMap<ConnId, SocketAddr>,
    queue_output: VecDeque<QueueOutput<SC, SE, TC>>,
    switcher: TaskSwitcher,
}

impl<SC, SE, TC, TW> DataPlane<SC, SE, TC, TW> {
    pub fn new(worker_id: u16, node_id: NodeId, services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>, history: Arc<dyn ShadowRouterHistory>) -> Self {
        log::info!("Create DataPlane for node: {}", node_id);

        Self {
            worker_id,
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
            switcher: TaskSwitcher::new(2),
        }
    }

    pub fn route(&self, rule: RouteRule, source: Option<NodeId>, relay_from: Option<NodeId>) -> RouteAction<SocketAddr> {
        self.feature_ctx.router.derive_action(&rule, source, relay_from)
    }

    pub fn on_tick<'a>(&mut self, now_ms: u64) {
        self.switcher.queue_flag_all();
        self.features.on_tick(&mut self.feature_ctx, now_ms, self.tick_count);
        self.services.on_tick(&mut self.service_ctx, now_ms, self.tick_count);
        self.tick_count += 1;
    }

    pub fn on_event<'a>(&mut self, now_ms: u64, event: Input<'a, SC, SE, TW>) -> Option<Output<'a, SC, SE, TC>> {
        match event {
            Input::Ext(ext) => match ext {
                ExtIn::ConnectTo(_remote) => {
                    panic!("ConnectTo is not supported")
                }
                ExtIn::DisconnectFrom(_node) => {
                    panic!("DisconnectFrom is not supported")
                }
                ExtIn::FeaturesControl(control) => {
                    let feature: Features = control.to_feature();
                    let actor = FeatureControlActor::Worker(self.worker_id);
                    let out = self.features.on_input(&mut self.feature_ctx, feature, now_ms, FeatureWorkerInput::Control(actor, control))?;
                    Some(self.convert_features(now_ms, feature, out))
                }
                ExtIn::ServicesControl(service, control) => {
                    let actor = ServiceControlActor::Worker(self.worker_id);
                    let out = self.services.on_input(&mut self.service_ctx, now_ms, service, ServiceWorkerInput::Control(actor, control))?;
                    Some(self.convert_services(now_ms, service, out))
                }
            },
            Input::Worker(CrossWorker::Feature(event)) => Some(Output::Ext(ExtOut::FeaturesEvent(event))),
            Input::Worker(CrossWorker::Service(service, event)) => Some(Output::Ext(ExtOut::ServicesEvent(service, event))),
            Input::Net(NetInput::UdpPacket(remote, buf)) => {
                if buf.is_empty() {
                    return None;
                }
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
            Input::Event(LogicEvent::ExtFeaturesEvent(worker, event)) => {
                assert_eq!(self.worker_id, worker);
                Some(Output::Ext(ExtOut::FeaturesEvent(event)))
            }
            Input::Event(LogicEvent::ExtServicesEvent(worker, service, event)) => {
                assert_eq!(self.worker_id, worker);
                Some(Output::Ext(ExtOut::ServicesEvent(service, event)))
            }
            Input::Event(LogicEvent::NetNeighbour(remote, control)) => {
                let buf: Result<Vec<u8>, _> = (&control).try_into();
                Some(NetOutput::UdpPacket(remote, Buffer::from(buf.ok()?)).into())
            }
            Input::Event(LogicEvent::NetDirect(feature, remote, _conn, meta, buf)) => {
                let header = meta.to_header(feature as u8, RouteRule::Direct, self.feature_ctx.node_id);
                let conn = self.conns.get_mut(&remote)?;
                let msg = TransportMsg::build_raw(header, &buf);
                Self::build_send_to_from_mut(now_ms, conn, remote, msg.take().into()).map(|e| e.into())
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

    pub fn pop_output<'a>(&mut self, now_ms: u64) -> Option<Output<'a, SC, SE, TC>> {
        if let Some(out) = self.queue_output.pop_front() {
            return match out {
                QueueOutput::Feature(feature, out) => Some(self.convert_features(now_ms, feature, out)),
                QueueOutput::Service(service, out) => Some(self.convert_services(now_ms, service, out)),
                QueueOutput::Net(out) => Some(Output::Net(out)),
            };
        };

        while let Some(current) = self.switcher.queue_current() {
            match current {
                0 => {
                    let out = self.features.pop_output(&mut self.feature_ctx);
                    if let Some((feature, out)) = self.switcher.queue_process(out) {
                        return Some(self.convert_features(now_ms, feature, out));
                    }
                }
                1 => {
                    let out = self.services.pop_output(&mut self.service_ctx);
                    if let Some((feature, out)) = self.switcher.queue_process(out) {
                        return Some(self.convert_services(now_ms, feature, out));
                    }
                }
                _ => panic!("Invalid task group index: {}", current),
            }
        }

        None
    }

    fn incoming_route<'a>(&mut self, now_ms: u64, remote: SocketAddr, mut buf: BufferMut<'a>) -> Option<Output<'a, SC, SE, TC>> {
        let conn = self.conns.get_mut(&remote)?;
        if TransportMsgHeader::is_secure(buf[0]) {
            conn.decrypt_if_need(now_ms, &mut buf)?;
        }
        let header = TransportMsgHeader::try_from(&buf as &[u8]).ok()?;
        let action = self.feature_ctx.router.derive_action(&header.route, header.from_node, Some(conn.node()));
        log::debug!("[DataPlame] Incoming rule: {:?} from: {remote}, node {:?} => action {:?}", header.route, header.from_node, action);
        match action {
            RouteAction::Reject => None,
            RouteAction::Local => {
                let feature = header.feature.try_into().ok()?;
                log::debug!("Incoming feature: {:?} from: {remote}", feature);
                let out = self.features.on_network_raw(&mut self.feature_ctx, feature, now_ms, conn.conn(), remote, header, buf.freeze())?;
                Some(self.convert_features(now_ms, feature, out))
            }
            RouteAction::Next(remote) => {
                if !TransportMsgHeader::decrease_ttl(&mut buf) {
                    log::debug!("TTL is 0, drop packet");
                }
                let target_conn = self.conns.get_mut(&remote)?;
                Self::build_send_to_from_mut(now_ms, target_conn, remote, buf).map(|e| e.into())
            }
            RouteAction::Broadcast(local, remotes) => {
                if !TransportMsgHeader::decrease_ttl(&mut buf) {
                    log::debug!("TTL is 0, drop packet");
                    return None;
                }
                if local {
                    if let Ok(feature) = header.feature.try_into() {
                        log::debug!("Incoming broadcast feature: {:?} from: {remote}", feature);
                        if let Some(out) = self.features.on_network_raw(&mut self.feature_ctx, feature, now_ms, conn.conn(), remote, header, buf.copy_readonly()) {
                            self.queue_output.push_back(QueueOutput::Feature(feature, out.owned()));
                        }
                    }
                }
                self.build_send_to_multi_from_mut(now_ms, remotes, buf).map(|e: NetOutput<'_>| e.into())
            }
        }
    }

    fn outgoing_route<'a>(&mut self, now_ms: u64, feature: Features, rule: RouteRule, mut meta: NetOutgoingMeta, buf: Vec<u8>) -> Option<Output<'a, SC, SE, TC>> {
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
                let conn = self.conns.get_mut(&remote)?;
                Self::build_send_to_from_mut(now_ms, conn, remote, msg.take().into()).map(|e| e.into())
            }
            RouteAction::Broadcast(local, remotes) => {
                log::debug!("[DataPlane] outgoing route rule {:?} is go with local {local} and remotes {:?}", rule, remotes);
                meta.source = true; //Force enable source for broadcast

                let header = meta.to_header(feature as u8, rule, self.feature_ctx.node_id);
                let msg = TransportMsg::build_raw(header, &buf);
                if local {
                    let meta = meta.to_incoming(self.feature_ctx.node_id);
                    if let Some(out) = self.features.on_input(&mut self.feature_ctx, feature, now_ms, FeatureWorkerInput::Local(meta, buf.into())) {
                        self.switcher.queue_flag_task(TaskType::Feature as usize);
                        self.queue_output.push_back(QueueOutput::Feature(feature, out.owned()));
                    }
                }
                self.build_send_to_multi_from_mut(now_ms, remotes, msg.take().into()).map(|e| e.into())
            }
        }
    }

    fn convert_features<'a>(&mut self, now_ms: u64, feature: Features, out: FeatureWorkerOutput<'a, FeaturesControl, FeaturesEvent, FeaturesToController>) -> Output<'a, SC, SE, TC> {
        self.switcher.queue_flag_task(TaskType::Feature as usize);

        match out {
            FeatureWorkerOutput::ForwardControlToController(service, control) => LogicControl::FeaturesControl(service, control).into(),
            FeatureWorkerOutput::ForwardNetworkToController(conn, header, msg) => LogicControl::NetRemote(feature, conn, header, msg).into(),
            FeatureWorkerOutput::ForwardLocalToController(header, buf) => LogicControl::NetLocal(feature, header, buf).into(),
            FeatureWorkerOutput::ToController(control) => LogicControl::Feature(control).into(),
            FeatureWorkerOutput::Event(actor, event) => match actor {
                FeatureControlActor::Controller => Output::Control(LogicControl::ExtFeaturesEvent(event)),
                FeatureControlActor::Worker(worker) => {
                    if self.worker_id == worker {
                        Output::Ext(ExtOut::FeaturesEvent(event))
                    } else {
                        Output::Worker(worker, CrossWorker::Feature(event))
                    }
                }
                FeatureControlActor::Service(service) => {
                    if let Some(out) = self.services.on_input(&mut self.service_ctx, now_ms, service, ServiceWorkerInput::FeatureEvent(event)) {
                        self.convert_services(now_ms, service, out)
                    } else {
                        Output::Continue
                    }
                }
            },
            FeatureWorkerOutput::SendDirect(conn, meta, buf) => {
                if let Some(addr) = self.conns_reverse.get(&conn) {
                    let conn = self.conns.get_mut(addr).expect("Should have");
                    let header = meta.to_header(feature as u8, RouteRule::Direct, self.feature_ctx.node_id);
                    let msg = TransportMsg::build_raw(header, &buf);
                    Self::build_send_to_from_mut(now_ms, conn, *addr, msg.take().into()).expect("Should have output").into()
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
                    let conn = self.conns.get_mut(addr).expect("Should have conn");
                    Self::build_send_to(now_ms, conn, *addr, buf).expect("Should ok for convert RawDirect").into()
                } else {
                    Output::Continue
                }
            }
            FeatureWorkerOutput::RawBroadcast(conns, buf) => {
                let addrs = conns.iter().filter_map(|conn| self.conns_reverse.get(conn)).cloned().collect();
                self.build_send_to_multi(now_ms, addrs, buf).map(|e| e.into()).unwrap_or(Output::Continue)
            }
            FeatureWorkerOutput::RawDirect2(addr, buf) => {
                if let Some(conn) = self.conns.get_mut(&addr) {
                    Self::build_send_to(now_ms, conn, addr, buf).expect("Should ok for convert RawDirect2").into()
                } else {
                    Output::Continue
                }
            }
            FeatureWorkerOutput::RawBroadcast2(addrs, buf) => self.build_send_to_multi(now_ms, addrs, buf).map(|e| e.into()).unwrap_or(Output::Continue),
            FeatureWorkerOutput::TunPkt(pkt) => NetOutput::TunPacket(pkt).into(),
        }
    }

    fn convert_services<'a>(&mut self, now_ms: u64, service: ServiceId, out: ServiceWorkerOutput<FeaturesControl, FeaturesEvent, SC, SE, TC>) -> Output<'a, SC, SE, TC> {
        self.switcher.queue_flag_task(TaskType::Service as usize);

        match out {
            ServiceWorkerOutput::ForwardControlToController(actor, control) => LogicControl::ServicesControl(actor, service, control).into(),
            ServiceWorkerOutput::ForwardFeatureEventToController(event) => LogicControl::ServiceEvent(service, event).into(),
            ServiceWorkerOutput::ToController(tc) => LogicControl::Service(service, tc).into(),
            ServiceWorkerOutput::FeatureControl(control) => {
                let feature = control.to_feature();
                if let Some(out) = self
                    .features
                    .on_input(&mut self.feature_ctx, feature, now_ms, FeatureWorkerInput::Control(FeatureControlActor::Service(service), control))
                {
                    self.convert_features(now_ms, feature, out)
                } else {
                    Output::Continue
                }
            }
            ServiceWorkerOutput::Event(actor, event) => match actor {
                ServiceControlActor::Controller => Output::Control(LogicControl::ExtServicesEvent(service, event)),
                ServiceControlActor::Worker(worker) => {
                    if self.worker_id == worker {
                        Output::Ext(ExtOut::ServicesEvent(service, event))
                    } else {
                        Output::Worker(worker, CrossWorker::Service(service, event))
                    }
                }
            },
        }
    }

    fn build_send_to_from_mut<'a>(now: u64, conn: &mut DataPlaneConnection, remote: SocketAddr, mut buf: BufferMut<'a>) -> Option<NetOutput<'a>> {
        conn.encrypt_if_need(now, &mut buf)?;
        let after = buf.freeze();
        Some(NetOutput::UdpPacket(remote, after))
    }

    fn build_send_to_multi_from_mut<'a>(&mut self, now: u64, mut remotes: Vec<SocketAddr>, mut buf: BufferMut<'a>) -> Option<NetOutput<'a>> {
        if TransportMsgHeader::is_secure(buf[0]) {
            let first = remotes.pop()?;
            for remote in remotes {
                if let Some(conn) = self.conns.get_mut(&remote) {
                    let mut buf = BufferMut::build(&buf, 0, 12 + 16);
                    if let Some(_) = conn.encrypt_if_need(now, &mut buf) {
                        let out = NetOutput::UdpPacket(remote, buf.freeze());
                        self.queue_output.push_back(QueueOutput::Net(out));
                    }
                }
            }
            let conn = self.conns.get_mut(&first)?;
            conn.encrypt_if_need(now, &mut buf)?;
            Some(NetOutput::UdpPacket(first, buf.freeze()))
        } else {
            Some(NetOutput::UdpPackets(remotes, buf.freeze()))
        }
    }

    fn build_send_to_multi<'a>(&mut self, now: u64, remotes: Vec<SocketAddr>, buf: Buffer<'a>) -> Option<NetOutput<'a>> {
        if TransportMsgHeader::is_secure(buf[0]) {
            let buf = BufferMut::build(&buf, 0, 12 + 16);
            self.build_send_to_multi_from_mut(now, remotes, buf)
        } else {
            Some(NetOutput::UdpPackets(remotes, buf))
        }
    }

    fn build_send_to<'a>(now: u64, conn: &mut DataPlaneConnection, remote: SocketAddr, buf: Buffer<'a>) -> Option<NetOutput<'a>> {
        if TransportMsgHeader::is_secure(buf[0]) {
            let buf = BufferMut::build(&buf, 0, 12 + 16);
            Self::build_send_to_from_mut(now, conn, remote, buf)
        } else {
            Some(NetOutput::UdpPacket(remote, buf))
        }
    }
}

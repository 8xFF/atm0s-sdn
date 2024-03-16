use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{shadow::ShadowRouter, RouteAction, RouteRule, RouterTable};

use crate::{
    base::{FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer, NeighboursControl, SecureContext, ServiceId, ServiceWorkerInput, ServiceWorkerOutput, TransportMsg},
    features::{FeaturesControl, FeaturesEvent, FeaturesToController, FeaturesToWorker},
    san_io_utils::TasksSwitcher,
};

use self::{connection::DataPlaneConnection, features::FeatureWorkerManager, services::ServiceWorkerManager};

mod connection;
mod features;
mod services;

#[derive(Debug, Clone)]
pub enum NetInput<'a> {
    UdpPacket(SocketAddr, GenericBuffer<'a>),
}

#[derive(Debug, Clone)]
pub enum BusInput<TW> {
    FromFeatureController(FeaturesToWorker),
    FromServiceController(ServiceId, TW),
    NeigboursControl(SocketAddr, NeighboursControl),
    NetDirect(u8, ConnId, Vec<u8>),
    NetRoute(u8, RouteRule, Vec<u8>),
    Pin(ConnId, SocketAddr, SecureContext),
    UnPin(ConnId),
}

#[derive(Debug, Clone)]
pub enum Input<'a, TW> {
    Net(NetInput<'a>),
    Bus(BusInput<TW>),
    ShutdownRequest,
}

#[derive(Debug, Clone)]
pub enum NetOutput<'a> {
    UdpPacket(SocketAddr, GenericBuffer<'a>),
    UdpPackets(Vec<SocketAddr>, GenericBuffer<'a>),
}

#[derive(Debug, Clone)]
pub enum BusOutput<TC> {
    ForwardControlToController(ServiceId, FeaturesControl),
    ForwardEventToController(ServiceId, FeaturesEvent),
    ForwardNetworkToController(u8, ConnId, Vec<u8>),
    ForwardLocalToController(u8, Vec<u8>),
    ToFeatureController(FeaturesToController),
    ToServiceController(ServiceId, TC),
    NeigboursControl(SocketAddr, NeighboursControl),
}

#[derive(convert_enum::From)]
pub enum Output<'a, TC> {
    Net(NetOutput<'a>),
    Bus(BusOutput<TC>),
    #[convert_enum(optout)]
    ShutdownResponse,
    #[convert_enum(optout)]
    Continue,
}

#[derive(Debug, Clone, Copy)]
enum TaskType {
    Feature,
    Service,
}

enum QueueOutput<TC> {
    Feature(u8, FeatureWorkerOutput<FeaturesControl, FeaturesEvent, FeaturesToController>),
    Service(ServiceId, ServiceWorkerOutput<FeaturesControl, FeaturesEvent, TC>),
}

pub struct DataPlane<TC, TW> {
    ctx: FeatureWorkerContext,
    features: FeatureWorkerManager,
    services: ServiceWorkerManager<TC, TW>,
    conns: HashMap<SocketAddr, DataPlaneConnection>,
    conns_reverse: HashMap<ConnId, SocketAddr>,
    queue_output: VecDeque<QueueOutput<TC>>,
    last_task: Option<TaskType>,
    switcher: TasksSwitcher<2>,
}

impl<TC, TW> DataPlane<TC, TW> {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            ctx: FeatureWorkerContext { router: ShadowRouter::new(node_id) },
            features: FeatureWorkerManager::new(),
            services: ServiceWorkerManager::new(),
            conns: HashMap::new(),
            conns_reverse: HashMap::new(),
            queue_output: VecDeque::new(),
            last_task: None,
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_tick<'a>(&mut self, now_ms: u64) {
        self.last_task = None;
        self.features.on_tick(&mut self.ctx, now_ms);
        self.services.on_tick(now_ms);
    }

    pub fn on_event<'a>(&mut self, now_ms: u64, event: Input<'a, TW>) -> Option<Output<'a, TC>> {
        match event {
            Input::Net(NetInput::UdpPacket(remote, buf)) => {
                if let Ok(control) = NeighboursControl::try_from(&*buf) {
                    Some(BusOutput::NeigboursControl(remote, control).into())
                } else {
                    self.incoming_route(now_ms, remote, TransportMsg::from_ref(&buf).ok()?)
                }
            }
            Input::Bus(BusInput::FromFeatureController(to)) => {
                let (feature, out) = self.features.on_input(&mut self.ctx, 0, now_ms, FeatureWorkerInput::FromController(to))?;
                Some(self.convert_features(now_ms, feature, out))
            }
            Input::Bus(BusInput::FromServiceController(service, to)) => {
                let out = self.services.on_input(now_ms, service, ServiceWorkerInput::FromController(to))?;
                Some(self.convert_services(now_ms, service, out))
            }
            Input::Bus(BusInput::NeigboursControl(remote, control)) => {
                let buf = (&control).try_into().ok()?;
                Some(NetOutput::UdpPacket(remote, GenericBuffer::Vec(buf)).into())
            }
            Input::Bus(BusInput::NetDirect(feature, conn, buf)) => {
                let addr = self.conns_reverse.get(&conn)?;
                let msg = TransportMsg::build(feature, 0, RouteRule::Direct, &buf);
                Some(NetOutput::UdpPacket(*addr, msg.take().into()).into())
            }
            Input::Bus(BusInput::NetRoute(feature, rule, buf)) => self.outgoing_route(now_ms, feature, rule, buf),
            Input::Bus(BusInput::Pin(conn, addr, secure)) => {
                self.conns.insert(addr, DataPlaneConnection::new(conn, addr, secure));
                self.conns_reverse.insert(conn, addr);
                None
            }
            Input::Bus(BusInput::UnPin(conn)) => {
                if let Some(addr) = self.conns_reverse.remove(&conn) {
                    log::info!("Pin: conn: {} <--> addr: {}", conn, addr);
                    self.conns.remove(&addr);
                }
                None
            }
            Input::ShutdownRequest => Some(Output::ShutdownResponse),
        }
    }

    pub fn pop_output<'a>(&mut self, now_ms: u64) -> Option<Output<'a, TC>> {
        if let Some(last) = &self.last_task {
            if let Some(out) = self.pop_last(now_ms, *last) {
                return Some(out);
            } else {
                self.last_task = None;
                match self.queue_output.pop_front()? {
                    QueueOutput::Feature(feature, out) => Some(self.convert_features(now_ms, feature, out)),
                    QueueOutput::Service(service, out) => Some(self.convert_services(now_ms, service, out)),
                }
            }
        } else {
            while let Some(current) = self.switcher.current() {
                match current {
                    0 => {
                        let out = self.pop_last(now_ms, TaskType::Feature);
                        if let Some(out) = self.switcher.process(out) {
                            return Some(out);
                        }
                    }
                    1 => {
                        let out = self.pop_last(now_ms, TaskType::Service);
                        if let Some(out) = self.switcher.process(out) {
                            return Some(out);
                        }
                    }
                    _ => return None,
                }
            }

            None
        }
    }

    fn pop_last<'a>(&mut self, now_ms: u64, last_task: TaskType) -> Option<Output<'a, TC>> {
        match last_task {
            TaskType::Feature => {
                let (feature, out) = self.features.pop_output()?;
                Some(self.convert_features(now_ms, feature, out))
            }
            TaskType::Service => {
                let (service, out) = self.services.pop_output()?;
                Some(self.convert_services(now_ms, service, out))
            }
        }
    }

    fn incoming_route<'a>(&mut self, now_ms: u64, remote: SocketAddr, msg: TransportMsg) -> Option<Output<'a, TC>> {
        match self.ctx.router.derive_action(&msg.header.route) {
            RouteAction::Reject => None,
            RouteAction::Local => {
                let conn = self.conns.get(&remote)?;
                let (feature, out) = self
                    .features
                    .on_input(&mut self.ctx, msg.header.feature, now_ms, FeatureWorkerInput::Network(conn.conn(), msg.payload().to_vec().into()))?;
                Some(self.convert_features(now_ms, feature, out))
            }
            RouteAction::Next(remote) => Some(NetOutput::UdpPacket(remote, msg.take().into()).into()),
            RouteAction::Broadcast(local, remotes) => {
                if local {
                    if let Some(conn) = self.conns.get(&remote) {
                        if let Some((feature, out)) = self
                            .features
                            .on_input(&mut self.ctx, msg.header.feature, now_ms, FeatureWorkerInput::Network(conn.conn(), msg.payload().to_vec().into()))
                        {
                            self.queue_output.push_back(QueueOutput::Feature(feature, out));
                        }
                    }
                }
                Some(NetOutput::UdpPackets(remotes, msg.take().into()).into())
            }
        }
    }

    fn outgoing_route<'a>(&mut self, now_ms: u64, feature: u8, rule: RouteRule, buf: Vec<u8>) -> Option<Output<'a, TC>> {
        match self.ctx.router.derive_action(&rule) {
            RouteAction::Reject => None,
            RouteAction::Local => {
                let (feature, out) = self.features.on_input(&mut self.ctx, feature, now_ms, FeatureWorkerInput::Local(buf))?;
                Some(self.convert_features(now_ms, feature, out))
            }
            RouteAction::Next(remote) => {
                let msg = TransportMsg::build(feature, 0, rule, &buf);
                Some(NetOutput::UdpPacket(remote, msg.take().into()).into())
            }
            RouteAction::Broadcast(local, remotes) => {
                let msg = TransportMsg::build(feature, 0, rule, &buf);
                if local {
                    if let Some((feature, out)) = self.features.on_input(&mut self.ctx, feature, now_ms, FeatureWorkerInput::Local((&buf).to_vec().into())) {
                        self.queue_output.push_back(QueueOutput::Feature(feature, out));
                    }
                }

                Some(NetOutput::UdpPackets(remotes, msg.take().into()).into())
            }
        }
    }

    fn convert_features<'a>(&mut self, now_ms: u64, feature: u8, out: FeatureWorkerOutput<FeaturesControl, FeaturesEvent, FeaturesToController>) -> Output<'a, TC> {
        self.last_task = Some(TaskType::Feature);
        match out {
            FeatureWorkerOutput::ForwardControlToController(service, control) => BusOutput::ForwardControlToController(service, control).into(),
            FeatureWorkerOutput::ForwardNetworkToController(conn, msg) => BusOutput::ForwardNetworkToController(feature, conn, msg).into(),
            FeatureWorkerOutput::ForwardLocalToController(buf) => BusOutput::ForwardLocalToController(feature, buf).into(),
            FeatureWorkerOutput::ToController(control) => BusOutput::ToFeatureController(control).into(),
            FeatureWorkerOutput::Event(service, event) => {
                if let Some(out) = self.services.on_input(now_ms, service, ServiceWorkerInput::FeatureEvent(event)) {
                    self.queue_output.push_back(QueueOutput::Service(service, out));
                }
                Output::Continue
            }
            FeatureWorkerOutput::SendDirect(conn, buf) => {
                if let Some(addr) = self.conns_reverse.get(&conn) {
                    let msg = TransportMsg::build(feature, 0, RouteRule::Direct, &buf);
                    NetOutput::UdpPacket(*addr, msg.take().into()).into()
                } else {
                    Output::Continue
                }
            }
            FeatureWorkerOutput::SendRoute(rule, buf) => {
                log::info!("SendRoute: {:?}", rule);
                if let Some(out) = self.outgoing_route(now_ms, feature, rule, buf) {
                    out
                } else {
                    Output::Continue
                }
            }
        }
    }

    fn convert_services<'a>(&mut self, now_ms: u64, service: ServiceId, out: ServiceWorkerOutput<FeaturesControl, FeaturesEvent, TC>) -> Output<'a, TC> {
        self.last_task = Some(TaskType::Service);
        match out {
            ServiceWorkerOutput::ForwardFeatureEventToController(event) => BusOutput::ForwardEventToController(service, event).into(),
            ServiceWorkerOutput::ToController(tc) => BusOutput::ToServiceController(service, tc).into(),
            ServiceWorkerOutput::FeatureControl(control) => {
                if let Some((feature, out)) = self.features.on_input(&mut self.ctx, 0, now_ms, FeatureWorkerInput::Control(service, control)) {
                    self.queue_output.push_back(QueueOutput::Feature(feature, out));
                }
                Output::Continue
            }
        }
    }
}

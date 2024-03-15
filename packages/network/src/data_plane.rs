use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{shadow::ShadowRouter, RouteAction, RouterTable};

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
    NetDirect(ConnId, TransportMsg),
    NetRoute(TransportMsg),
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
    ForwardNetworkToController(ConnId, TransportMsg),
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
    Feature(FeatureWorkerOutput<FeaturesControl, FeaturesEvent, FeaturesToController>),
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
                    self.process_route(now_ms, TransportMsg::from_ref(&buf).ok()?)
                }
            }
            Input::Bus(BusInput::FromFeatureController(to)) => {
                let out = self.features.on_input(&mut self.ctx, now_ms, FeatureWorkerInput::FromController(to))?;
                Some(self.convert_features(now_ms, out))
            }
            Input::Bus(BusInput::FromServiceController(service, to)) => {
                let out = self.services.on_input(now_ms, service, ServiceWorkerInput::FromController(to))?;
                Some(self.convert_services(now_ms, service, out))
            }
            Input::Bus(BusInput::NeigboursControl(remote, control)) => {
                let buf = (&control).try_into().ok()?;
                Some(NetOutput::UdpPacket(remote, GenericBuffer::Vec(buf)).into())
            }
            Input::Bus(BusInput::NetDirect(conn, msg)) => {
                let addr = self.conns_reverse.get(&conn)?;
                Some(NetOutput::UdpPacket(*addr, msg.take().into()).into())
            }
            Input::Bus(BusInput::NetRoute(msg)) => self.process_route(now_ms, msg),
            Input::Bus(BusInput::Pin(conn, addr, secure)) => {
                self.conns.insert(addr, DataPlaneConnection::new(conn, addr, secure));
                self.conns_reverse.insert(conn, addr);
                None
            }
            Input::Bus(BusInput::UnPin(conn)) => {
                if let Some(addr) = self.conns_reverse.remove(&conn) {
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
                    QueueOutput::Feature(out) => Some(self.convert_features(now_ms, out)),
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
                let out = self.features.pop_output()?;
                Some(self.convert_features(now_ms, out))
            }
            TaskType::Service => {
                let (service, out) = self.services.pop_output()?;
                Some(self.convert_services(now_ms, service, out))
            }
        }
    }

    fn process_route<'a>(&mut self, now_ms: u64, msg: TransportMsg) -> Option<Output<'a, TC>> {
        match self.ctx.router.derive_action(&msg.header.route) {
            RouteAction::Reject => None,
            RouteAction::Local => {
                let out = self.features.on_input(&mut self.ctx, now_ms, FeatureWorkerInput::Local(msg))?;
                Some(self.convert_features(now_ms, out))
            }
            RouteAction::Next(remote) => Some(NetOutput::UdpPacket(remote, msg.take().into()).into()),
            RouteAction::Broadcast(local, remotes) => {
                if local {
                    if let Some(out) = self.features.on_input(&mut self.ctx, now_ms, FeatureWorkerInput::Local(msg.clone())) {
                        self.queue_output.push_back(QueueOutput::Feature(out));
                    }
                }
                Some(NetOutput::UdpPackets(remotes, msg.take().into()).into())
            }
        }
    }

    fn convert_features<'a>(&mut self, now_ms: u64, out: FeatureWorkerOutput<FeaturesControl, FeaturesEvent, FeaturesToController>) -> Output<'a, TC> {
        self.last_task = Some(TaskType::Feature);
        match out {
            FeatureWorkerOutput::ForwardControlToController(service, control) => BusOutput::ForwardControlToController(service, control).into(),
            FeatureWorkerOutput::ForwardNetworkToController(conn, msg) => BusOutput::ForwardNetworkToController(conn, msg).into(),
            FeatureWorkerOutput::ToController(control) => BusOutput::ToFeatureController(control).into(),
            FeatureWorkerOutput::Event(service, event) => {
                if let Some(out) = self.services.on_input(now_ms, service, ServiceWorkerInput::FeatureEvent(event)) {
                    self.queue_output.push_back(QueueOutput::Service(service, out));
                }
                Output::Continue
            }
            FeatureWorkerOutput::SendDirect(conn, msg) => {
                if let Some(addr) = self.conns_reverse.get(&conn) {
                    NetOutput::UdpPacket(*addr, msg.take().into()).into()
                } else {
                    Output::Continue
                }
            }
            FeatureWorkerOutput::SendRoute(msg) => {
                if let Some(out) = self.process_route(now_ms, msg) {
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
                if let Some(out) = self.features.on_input(&mut self.ctx, now_ms, FeatureWorkerInput::Control(service, control)) {
                    self.queue_output.push_back(QueueOutput::Feature(out));
                }
                Output::Continue
            }
        }
    }
}

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

pub enum Input<'a, TW> {
    UdpPacket(SocketAddr, GenericBuffer<'a>),
    FromFeatureController(FeaturesToWorker),
    FromServiceController(ServiceId, TW),
    NeigboursControl(SocketAddr, NeighboursControl),
    NetDirect(ConnId, TransportMsg),
    NetRoute(TransportMsg),
    Pin(ConnId, SocketAddr, SecureContext),
    UnPin(ConnId),
    ShutdownRequest,
}

pub enum Output<'a, TC> {
    UdpPacket(SocketAddr, GenericBuffer<'a>),
    UdpPackets(Vec<SocketAddr>, GenericBuffer<'a>),
    ForwardControlToController(ServiceId, FeaturesControl),
    ForwardEventToController(ServiceId, FeaturesEvent),
    ForwardNetworkToController(ConnId, TransportMsg),
    ToFeatureController(FeaturesToController),
    ToServiceController(ServiceId, TC),
    NeigboursControl(SocketAddr, NeighboursControl),
    ShutdownResponse,
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
        // Self {
        //     ctx: FeatureWorkerContext {
        //         router: ShadowRouter::new(node_id),
        //     },
        // }
        todo!()
    }

    pub fn on_tick<'a>(&mut self, now_ms: u64) {
        self.last_task = None;
        self.features.on_tick(&mut self.ctx, now_ms);
        self.services.on_tick(now_ms);
    }

    pub fn on_event<'a>(&mut self, now_ms: u64, event: Input<'a, TW>) -> Option<Output<'a, TC>> {
        match event {
            Input::UdpPacket(remote, buf) => {
                if let Ok(control) = NeighboursControl::try_from(&*buf) {
                    Some(Output::NeigboursControl(remote, control))
                } else {
                    self.process_route(now_ms, TransportMsg::from_ref(&buf).ok()?)
                }
            }
            Input::FromFeatureController(to) => {
                let out = self.features.on_input(&mut self.ctx, now_ms, FeatureWorkerInput::FromController(to))?;
                self.convert_features(now_ms, out)
            }
            Input::FromServiceController(service, to) => {
                let out = self.services.on_input(now_ms, service, ServiceWorkerInput::FromController(to))?;
                self.convert_services(now_ms, service, out)
            }
            Input::NeigboursControl(remote, control) => {
                let buf = (&control).try_into().ok()?;
                Some(Output::UdpPacket(remote, GenericBuffer::Vec(buf)))
            }
            Input::NetDirect(conn, msg) => {
                let addr = self.conns_reverse.get(&conn)?;
                Some(Output::UdpPacket(*addr, msg.take().into()))
            }
            Input::NetRoute(msg) => self.process_route(now_ms, msg),
            Input::Pin(conn, addr, secure) => {
                self.conns.insert(addr, DataPlaneConnection::new(conn, addr, secure));
                self.conns_reverse.insert(conn, addr);
                None
            }
            Input::UnPin(conn) => {
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
                    QueueOutput::Feature(out) => self.convert_features(now_ms, out),
                    QueueOutput::Service(service, out) => self.convert_services(now_ms, service, out),
                }
            }
        } else {
            while let Some(current) = self.switcher.current() {
                match current {
                    0 => {
                        if let Some(out) = self.pop_last(now_ms, TaskType::Feature) {
                            return Some(out);
                        }
                    }
                    1 => {
                        if let Some(out) = self.pop_last(now_ms, TaskType::Service) {
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
                self.convert_features(now_ms, out)
            }
            TaskType::Service => {
                let (service, out) = self.services.pop_output()?;
                self.convert_services(now_ms, service, out)
            }
        }
    }

    fn process_route<'a>(&mut self, now_ms: u64, msg: TransportMsg) -> Option<Output<'a, TC>> {
        match self.ctx.router.derive_action(&msg.header.route) {
            RouteAction::Reject => None,
            RouteAction::Local => {
                let out = self.features.on_input(&mut self.ctx, now_ms, FeatureWorkerInput::Local(msg))?;
                self.convert_features(now_ms, out)
            }
            RouteAction::Next(remote) => Some(Output::UdpPacket(remote, msg.take().into())),
            RouteAction::Broadcast(local, remotes) => {
                if local {
                    if let Some(out) = self.features.on_input(&mut self.ctx, now_ms, FeatureWorkerInput::Local(msg.clone())) {
                        self.queue_output.push_back(QueueOutput::Feature(out));
                    }
                }
                Some(Output::UdpPackets(remotes, msg.take().into()))
            }
        }
    }

    fn convert_features<'a>(&mut self, now_ms: u64, out: FeatureWorkerOutput<FeaturesControl, FeaturesEvent, FeaturesToController>) -> Option<Output<'a, TC>> {
        let out = match out {
            FeatureWorkerOutput::ForwardControlToController(service, control) => Some(Output::ForwardControlToController(service, control)),
            FeatureWorkerOutput::ForwardNetworkToController(conn, msg) => Some(Output::ForwardNetworkToController(conn, msg)),
            FeatureWorkerOutput::ToController(control) => Some(Output::ToFeatureController(control)),
            FeatureWorkerOutput::Event(service, event) => self
                .services
                .on_input(now_ms, service, ServiceWorkerInput::FeatureEvent(event))
                .map(|o| self.convert_services(now_ms, service, o))
                .flatten(),
            FeatureWorkerOutput::SendDirect(conn, msg) => self.conns_reverse.get(&conn).map(|addr| Output::UdpPacket(*addr, msg.take().into())),
            FeatureWorkerOutput::SendRoute(msg) => self.process_route(now_ms, msg),
        };
        if out.is_some() {
            out
        } else {
            let out = self.features.pop_output()?;
            self.convert_features(now_ms, out)
        }
    }

    fn convert_services<'a>(&mut self, now_ms: u64, service: ServiceId, out: ServiceWorkerOutput<FeaturesControl, FeaturesEvent, TC>) -> Option<Output<'a, TC>> {
        let out = match out {
            ServiceWorkerOutput::ForwardFeatureEventToController(event) => Some(Output::ForwardEventToController(service, event)),
            ServiceWorkerOutput::ToController(tc) => Some(Output::ToServiceController(service, tc)),
            ServiceWorkerOutput::FeatureControl(control) => self
                .features
                .on_input(&mut self.ctx, now_ms, FeatureWorkerInput::Control(service, control))
                .map(|o| self.convert_features(now_ms, o))
                .flatten(),
        };
        if out.is_some() {
            out
        } else {
            let (service, out) = self.services.pop_output()?;
            self.convert_services(now_ms, service, out)
        }
    }
}

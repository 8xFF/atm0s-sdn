use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};

use crate::{
    base::{FeatureInput, FeatureOutput, FeatureSharedInput, NeighboursControl, SecureContext, ServiceId, ServiceInput, ServiceOutput, ServiceSharedInput, TransportMsg},
    features::{FeaturesControl, FeaturesEvent, FeaturesToController, FeaturesToWorker},
    san_io_utils::TasksSwitcher,
};

use self::{features::FeatureManager, neighbours::NeighboursManager, services::ServiceManager};

mod features;
mod neighbours;
mod services;

#[derive(Debug, Clone)]
pub enum BusIn<TC> {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    NeigboursControl(SocketAddr, NeighboursControl),
    FromFeatureWorker(FeaturesToController),
    FromServiceWorker(ServiceId, TC),
    ForwardNetFromWorker(ConnId, TransportMsg),
    ForwardControlFromWorker(ServiceId, FeaturesControl),
    ForwardEventFromWorker(ServiceId, FeaturesEvent),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum Input<TC> {
    Bus(BusIn<TC>),
    #[convert_enum(optout)]
    ShutdownRequest,
}

#[derive(Debug, Clone)]
pub enum BusOutSingle {
    NeigboursControl(SocketAddr, NeighboursControl),
    NetDirect(ConnId, TransportMsg),
    NetRoute(TransportMsg),
}

#[derive(Debug, Clone)]
pub enum BusOutMultiple<TW> {
    Pin(ConnId, SocketAddr, SecureContext),
    UnPin(ConnId),
    ToFeatureWorkers(FeaturesToWorker),
    ToServiceWorkers(ServiceId, TW),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum BusOut<TW> {
    Single(BusOutSingle),
    Multiple(BusOutMultiple<TW>),
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum Output<TW> {
    Bus(BusOut<TW>),
    #[convert_enum(optout)]
    ShutdownSuccess,
}

const NEIGHBOURS_ID: u8 = 0;
const FEATURES_ID: u8 = 1;
const SERVICES_ID: u8 = 2;

#[repr(u8)]
enum TaskType {
    Neighbours = NEIGHBOURS_ID,
    Feature = FEATURES_ID,
    Service = SERVICES_ID,
}

pub struct ControllerPlane<TC, TW> {
    neighbours: NeighboursManager,
    features: FeatureManager,
    services: ServiceManager<TC, TW>,
    // TODO may be we need stack style for optimize performance
    // and support some case task output call other task
    last_task: Option<TaskType>,
    switcher: TasksSwitcher<3>,
}

impl<TC, TW> ControllerPlane<TC, TW> {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            neighbours: NeighboursManager::new(node_id),
            features: FeatureManager::new(),
            services: ServiceManager::new(),
            last_task: None,
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.last_task = None;
        self.neighbours.on_tick(now_ms);
        self.features.on_input(now_ms, FeatureInput::Shared(FeatureSharedInput::Tick(now_ms)));
        self.services.on_shared_input(now_ms, ServiceSharedInput::Tick(now_ms));
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input<TC>) {
        match event {
            Input::Bus(BusIn::ConnectTo(addr)) => {
                self.last_task = Some(TaskType::Neighbours);
                self.neighbours.on_input(neighbours::Input::ConnectTo(addr));
            }
            Input::Bus(BusIn::DisconnectFrom(node)) => {
                self.last_task = Some(TaskType::Neighbours);
                self.neighbours.on_input(neighbours::Input::DisconnectFrom(node));
            }
            Input::Bus(BusIn::NeigboursControl(remote, control)) => {
                self.last_task = Some(TaskType::Neighbours);
                self.neighbours.on_input(neighbours::Input::Control(remote, control));
            }
            Input::Bus(BusIn::FromFeatureWorker(to)) => {
                self.last_task = Some(TaskType::Feature);
                self.features.on_input(now_ms, FeatureInput::FromWorker(to));
            }
            Input::Bus(BusIn::FromServiceWorker(service, to)) => {
                self.last_task = Some(TaskType::Service);
                self.services.on_input(now_ms, service, ServiceInput::FromWorker(to));
            }
            Input::Bus(BusIn::ForwardNetFromWorker(conn, msg)) => {
                self.last_task = Some(TaskType::Feature);
                if let Some(ctx) = self.neighbours.conn(conn) {
                    self.features.on_input(now_ms, FeatureInput::ForwardNetFromWorker(ctx, msg));
                }
            }
            Input::Bus(BusIn::ForwardEventFromWorker(service, event)) => {
                self.last_task = Some(TaskType::Service);
                self.services.on_input(now_ms, service, ServiceInput::FeatureEvent(event));
            }
            Input::Bus(BusIn::ForwardControlFromWorker(service, control)) => {
                self.last_task = Some(TaskType::Feature);
                self.features.on_input(now_ms, FeatureInput::Control(service, control));
            }
            Input::ShutdownRequest => {
                self.last_task = None;
                todo!()
            }
        }
    }

    pub fn pop_output(&mut self, now_ms: u64) -> Option<Output<TW>> {
        if let Some(last_task) = &self.last_task {
            match last_task {
                TaskType::Neighbours => self.pop_neighbours(now_ms),
                TaskType::Feature => self.pop_features(now_ms),
                TaskType::Service => self.pop_services(now_ms),
            }
        } else {
            while let Some(current) = self.switcher.current() {
                match current as u8 {
                    NEIGHBOURS_ID => {
                        let out = self.pop_neighbours(now_ms);
                        if let Some(out) = self.switcher.process(out) {
                            return Some(out);
                        }
                    }
                    FEATURES_ID => {
                        let out = self.pop_features(now_ms);
                        if let Some(out) = self.switcher.process(out) {
                            return Some(out);
                        }
                    }
                    SERVICES_ID => {
                        let out = self.pop_services(now_ms);
                        if let Some(out) = self.switcher.process(out) {
                            return Some(out);
                        }
                    }
                    _ => panic!("Should not happend!"),
                }
            }
            None
        }
    }

    fn pop_neighbours(&mut self, now_ms: u64) -> Option<Output<TW>> {
        let out = self.neighbours.pop_output()?;
        match out {
            neighbours::Output::Control(remote, control) => {
                return Some(Output::Bus(BusOut::Single(BusOutSingle::NeigboursControl(remote, control))));
            }
            neighbours::Output::Event(event) => {
                todo!("Process event here")
            }
        }
    }

    fn pop_features(&mut self, now_ms: u64) -> Option<Output<TW>> {
        let out = self.features.pop_output()?;
        match out {
            FeatureOutput::BroadcastToWorkers(to) => Some(Output::Bus(BusOut::Multiple(BusOutMultiple::ToFeatureWorkers(to)))),
            FeatureOutput::Event(service, event) => {
                //TODO may be we need stack style for optimize performance
                self.services.on_input(now_ms, service, ServiceInput::FeatureEvent(event));
                None
            }
            FeatureOutput::SendDirect(conn, msg) => Some(Output::Bus(BusOut::Single(BusOutSingle::NetDirect(conn, msg)))),
            FeatureOutput::SendRoute(msg) => Some(Output::Bus(BusOut::Single(BusOutSingle::NetRoute(msg)))),
            FeatureOutput::NeighboursConnectTo(addr) => {
                //TODO may be we need stack style for optimize performance
                self.neighbours.on_input(neighbours::Input::ConnectTo(addr));
                None
            }
            FeatureOutput::NeighboursDisconnectFrom(node) => {
                //TODO may be we need stack style for optimize performance
                self.neighbours.on_input(neighbours::Input::DisconnectFrom(node));
                None
            }
        }
    }

    fn pop_services(&mut self, now_ms: u64) -> Option<Output<TW>> {
        let (service, out) = self.services.pop_output()?;
        match out {
            ServiceOutput::FeatureControl(control) => {
                self.features.on_input(now_ms, FeatureInput::Control(service, control));
                None
            }
            ServiceOutput::BroadcastWorkers(to) => Some(Output::Bus(BusOut::Multiple(BusOutMultiple::ToServiceWorkers(service, to)))),
        }
    }
}

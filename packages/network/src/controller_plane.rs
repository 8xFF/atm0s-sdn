use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};

use crate::{
    base::{FeatureInput, FeatureOutput, FeatureSharedInput, NeighboursControl, SecureContext, ServiceId, ServiceInput, ServiceOutput, ServiceSharedInput, TransportMsg},
    features::{FeaturesToController, FeaturesToWorker},
    san_io_utils::TasksSwitcher,
};

use self::{features::FeatureManager, neighbours::NeighboursManager, services::ServiceManager};

mod features;
mod neighbours;
mod services;

pub enum Input<TC> {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    NeigboursControl(SocketAddr, NeighboursControl),
    FromFeatureWorker(FeaturesToController),
    FromServiceWorker(ServiceId, TC),
    ShutdownRequest,
}

pub enum Output<TW> {
    NeigboursControl(SocketAddr, NeighboursControl),
    NetDirect(ConnId, TransportMsg),
    NetRoute(TransportMsg),
    Pin(ConnId, SocketAddr, SecureContext),
    UnPin(ConnId),
    ToFeatureWorkers(FeaturesToWorker),
    ToServiceWorkers(ServiceId, TW),
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
            neighbours: NeighboursManager::new(),
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
            Input::ConnectTo(addr) => {
                self.last_task = Some(TaskType::Neighbours);
                self.neighbours.on_input(neighbours::Input::ConnectTo(addr));
            }
            Input::DisconnectFrom(node) => {
                self.last_task = Some(TaskType::Neighbours);
                self.neighbours.on_input(neighbours::Input::DisconnectFrom(node));
            }
            Input::NeigboursControl(remote, control) => {
                self.last_task = Some(TaskType::Neighbours);
                self.neighbours.on_input(neighbours::Input::Control(remote, control));
            }
            Input::FromFeatureWorker(to) => {
                self.last_task = Some(TaskType::Feature);
                self.features.on_input(now_ms, FeatureInput::FromWorker(to));
            }
            Input::FromServiceWorker(service, to) => {
                self.last_task = Some(TaskType::Service);
                self.services.on_input(now_ms, service, ServiceInput::FromWorker(to));
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
                return Some(Output::NeigboursControl(remote, control));
            }
            neighbours::Output::Event(event) => {
                todo!("Process event here")
            }
        }
    }

    fn pop_features(&mut self, now_ms: u64) -> Option<Output<TW>> {
        let out = self.features.pop_output()?;
        match out {
            FeatureOutput::BroadcastToWorkers(to) => Some(Output::ToFeatureWorkers(to)),
            FeatureOutput::Event(service, event) => {
                //TODO may be we need stack style for optimize performance
                self.services.on_input(now_ms, service, ServiceInput::FeatureEvent(event));
                None
            }
            FeatureOutput::SendDirect(conn, msg) => Some(Output::NetDirect(conn, msg)),
            FeatureOutput::SendRoute(msg) => Some(Output::NetRoute(msg)),
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
            ServiceOutput::BroadcastWorkers(to) => Some(Output::ToServiceWorkers(service, to)),
        }
    }
}

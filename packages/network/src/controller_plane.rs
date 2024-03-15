use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};

use crate::{
    base::{FeatureInput, FeatureSharedInput, SecureContext, ServiceId, ServiceSharedInput},
    features::{FeaturesToController, FeaturesToWorker},
};

use self::{
    features::FeatureManager,
    neighbours::{NeighboursControl, NeighboursManager},
    services::ServiceManager,
};

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
    Pin(ConnId, SocketAddr, SecureContext),
    UnPin(ConnId),
    ToFeatureWorker(FeaturesToWorker),
    ToServiceWorker(ServiceId, TW),
    ShutdownSuccess,
}

enum TaskType {
    Neighbours,
    Feature,
    Service,
}

pub struct ControllerPlane<TC, TW> {
    neighbours: NeighboursManager,
    features: FeatureManager,
    services: ServiceManager<TC, TW>,
    last_task: Option<TaskType>,
}

impl<TC, TW> ControllerPlane<TC, TW> {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            neighbours: NeighboursManager::new(),
            features: FeatureManager::new(),
            services: ServiceManager::new(),
            last_task: None,
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.last_task = None;
        self.neighbours.on_tick(now_ms);
        self.features.on_input(now_ms, FeatureInput::Shared(FeatureSharedInput::Tick(now_ms)));
        self.services.on_shared_input(now_ms, ServiceSharedInput::Tick(now_ms));
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input<TC>) {}

    pub fn pop_output(&mut self, now_ms: u64) -> Option<Output<TW>> {
        None
    }
}

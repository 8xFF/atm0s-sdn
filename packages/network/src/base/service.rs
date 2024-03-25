use atm0s_sdn_identity::NodeId;
use atm0s_sdn_utils::simple_pub_type;

use super::ConnectionEvent;

simple_pub_type!(ServiceId, u8);

/// First part is Service, which is running inside the controller.

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum ServiceControlActor {
    Controller,
}

#[derive(Debug, Clone)]
pub enum ServiceSharedInput {
    Tick(u64),
    Connection(ConnectionEvent),
}

#[derive(Debug)]
pub enum ServiceInput<FeaturesEvent, ServiceControl, ToController> {
    Control(ServiceControlActor, ServiceControl),
    FromWorker(ToController),
    FeatureEvent(FeaturesEvent),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServiceOutput<FeaturesControl, ServiceEvent, ToWorker> {
    Event(ServiceControlActor, ServiceEvent),
    FeatureControl(FeaturesControl),
    BroadcastWorkers(ToWorker),
}

pub struct ServiceCtx {
    pub node_id: NodeId,
    pub session: u64,
}

pub trait Service<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker> {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;

    fn on_shared_input<'a>(&mut self, _ctx: &ServiceCtx, _now: u64, _input: ServiceSharedInput);
    fn on_input(&mut self, _ctx: &ServiceCtx, _now: u64, input: ServiceInput<FeaturesEvent, ServiceControl, ToController>);
    fn pop_output(&mut self, _ctx: &ServiceCtx) -> Option<ServiceOutput<FeaturesControl, ServiceEvent, ToWorker>>;
}

/// Second part is Worker, which is running inside each data plane workers.

pub enum ServiceWorkerInput<FeaturesEvent, ToWorker> {
    FromController(ToWorker),
    FeatureEvent(FeaturesEvent),
}

pub enum ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ServiceEvent, ToController> {
    ForwardFeatureEventToController(FeaturesEvent),
    ToController(ToController),
    FeatureControl(FeaturesControl),
    Event(ServiceControlActor, ServiceEvent),
}

pub struct ServiceWorkerCtx {
    pub node_id: NodeId,
}

pub trait ServiceWorker<FeaturesControl, FeaturesEvent, ServiceEvent, ToController, ToWorker> {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;
    fn on_tick(&mut self, _ctx: &ServiceWorkerCtx, _now: u64, _tick_count: u64) {}
    fn on_input(
        &mut self,
        _ctx: &ServiceWorkerCtx,
        _now: u64,
        input: ServiceWorkerInput<FeaturesEvent, ToWorker>,
    ) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ServiceEvent, ToController>> {
        match input {
            ServiceWorkerInput::FeatureEvent(event) => Some(ServiceWorkerOutput::ForwardFeatureEventToController(event)),
            ServiceWorkerInput::FromController(_) => {
                log::warn!("No handler for FromController in {}", self.service_name());
                None
            }
        }
    }
    fn pop_output(&mut self, _ctx: &ServiceWorkerCtx) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ServiceEvent, ToController>> {
        None
    }
}

pub trait ServiceBuilder<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>: Send + Sync {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;
    fn discoverable(&self) -> bool {
        true
    }
    fn create(&self) -> Box<dyn Service<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>;
    fn create_worker(&self) -> Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, ServiceEvent, ToController, ToWorker>>;
}

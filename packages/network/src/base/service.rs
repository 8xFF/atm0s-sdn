use atm0s_sdn_utils::simple_pub_type;

use super::{ConnectionCtx, ConnectionStats, TransportMsg};

simple_pub_type!(ServiceId, u8);

/// First part is Service, which is running inside the controller.

#[derive(Debug, Clone)]
pub enum ServiceSharedInput<'a> {
    Tick(u64),
    ConnectionConnected(&'a ConnectionCtx),
    ConnectionStats(&'a ConnectionCtx, &'a ConnectionStats),
    ConnectionData(&'a ConnectionCtx, TransportMsg),
    ConnectionDisconnected(&'a ConnectionCtx),
}

pub enum ServiceInput<FeaturesEvent, ToController> {
    FromWorker(ToController),
    FeatureEvent(FeaturesEvent),
}

pub enum ServiceOutput<FeaturesControl, ToWorker> {
    FeatureControl(FeaturesControl),
    BroadcastWorkers(ToWorker),
}

pub trait Service<FeaturesControl, FeaturesEvent, ToController, ToWorker> {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;
    fn discoverable(&self) -> bool {
        true
    }

    fn on_shared_input<'a>(&mut self, _now: u64, _input: ServiceSharedInput<'a>);
    fn on_input(&mut self, _now: u64, input: ServiceInput<FeaturesEvent, ToController>);
    fn pop_output(&mut self) -> Option<ServiceOutput<FeaturesControl, ToWorker>>;
}

/// Second part is Worker, which is running inside each data plane workers.

pub enum ServiceWorkerInput<FeaturesEvent, ToWorker> {
    FromController(ToWorker),
    FeatureEvent(FeaturesEvent),
}

pub enum ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ToController> {
    ForwardFeatureEventToController(FeaturesEvent),
    ToController(ToController),
    FeatureControl(FeaturesControl),
}

pub trait ServiceWorker<FeaturesControl, FeaturesEvent, ToController, ToWorker> {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;
    fn on_tick(&mut self, _now: u64);
    fn on_input(&mut self, _now: u64, input: ServiceWorkerInput<FeaturesEvent, ToWorker>) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ToController>>;
    fn pop_output(&mut self) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ToController>>;
}

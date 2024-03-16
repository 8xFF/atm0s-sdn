use crate::{
    base::{Service, ServiceInput, ServiceOutput, ServiceWorker, ServiceWorkerInput, ServiceWorkerOutput},
    features::{FeaturesControl, FeaturesEvent},
};

pub const SERVICE_ID: u8 = 0;
pub const SERVICE_NAME: &str = "manual_discovery";

pub struct ManualDiscoveryService {}

impl<TC, TW> Service<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryService {
    fn service_id(&self) -> u8 {
        todo!()
    }

    fn service_name(&self) -> &str {
        todo!()
    }

    fn on_shared_input<'a>(&mut self, _now: u64, _input: crate::base::ServiceSharedInput) {
        todo!()
    }

    fn on_input(&mut self, _now: u64, input: ServiceInput<FeaturesEvent, TC>) {
        todo!()
    }

    fn pop_output(&mut self) -> Option<ServiceOutput<FeaturesControl, TW>> {
        todo!()
    }
}

pub struct ManualDiscoveryServiceWorker {}

impl<TC, TW> ServiceWorker<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryServiceWorker {
    fn service_id(&self) -> u8 {
        todo!()
    }

    fn service_name(&self) -> &str {
        todo!()
    }

    fn on_input(&mut self, _now: u64, input: ServiceWorkerInput<FeaturesEvent, TW>) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, TC>> {
        todo!()
    }

    fn pop_output(&mut self) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, TC>> {
        todo!()
    }
}

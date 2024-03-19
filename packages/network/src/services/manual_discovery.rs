use std::fmt::Debug;

use crate::{
    base::{Service, ServiceBuilder, ServiceInput, ServiceOutput, ServiceSharedInput, ServiceWorker},
    features::{FeaturesControl, FeaturesEvent},
};

pub const SERVICE_ID: u8 = 0;
pub const SERVICE_NAME: &str = "manual_discovery";

pub struct ManualDiscoveryService {}

impl<TC: Debug, TW: Debug> Service<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryService {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn on_shared_input<'a>(&mut self, _now: u64, input: ServiceSharedInput) {
        log::info!("ManualDiscoveryService received shared input: {:?}", input);
    }

    fn on_input(&mut self, _now: u64, input: ServiceInput<FeaturesEvent, TC>) {
        log::info!("ManualDiscoveryService received input: {:?}", input);
    }

    fn pop_output(&mut self) -> Option<ServiceOutput<FeaturesControl, TW>> {
        None
    }
}

pub struct ManualDiscoveryServiceWorker {}

impl<TC, TW> ServiceWorker<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryServiceWorker {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }
}

pub struct ManualDiscoveryServiceBuilder<TC, TW> {
    _tmp: std::marker::PhantomData<(TC, TW)>,
}

impl<TC, TW> Default for ManualDiscoveryServiceBuilder<TC, TW> {
    fn default() -> Self {
        Self { _tmp: std::marker::PhantomData }
    }
}

impl<TC: Debug + Send + Sync, TW: Debug + Send + Sync> ServiceBuilder<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryServiceBuilder<TC, TW> {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn create(&self) -> Box<dyn Service<FeaturesControl, FeaturesEvent, TC, TW>> {
        Box::new(ManualDiscoveryService {})
    }

    fn create_worker(&self) -> Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, TC, TW>> {
        Box::new(ManualDiscoveryServiceWorker {})
    }
}

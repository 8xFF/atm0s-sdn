pub const SERVICE_ID: u8 = 0;
pub const SERVICE_NAME: &str = "router_sync";

use super::{Service, ServiceWorker, ServiceWorkerInput, ServiceWorkerOutput};

pub struct ManualDiscoveryService {

}

impl<TC, TW> Service<TC, TW> for ManualDiscoveryService {
    fn service_id(&self) -> u8 {
        todo!()
    }

    fn service_name(&self) -> &str {
        todo!()
    }

    fn on_input(&mut self, _now: u64, input: super::ServiceInput<TC>) {
        todo!()
    }

    fn pop_output(&mut self) -> Option<super::ServiceOutput<TW>> {
        todo!()
    }
}

pub struct ManualDiscoveryServiceWorker {

}

impl<TC, TW> ServiceWorker<TC, TW> for ManualDiscoveryServiceWorker {
    fn service_id(&self) -> u8 {
        todo!()
    }

    fn service_name(&self) -> &str {
        todo!()
    }

    fn on_input(&mut self, _now: u64, input: ServiceWorkerInput<TW>) -> Option<ServiceWorkerOutput<TC>> {
        todo!()
    }

    fn pop_output(&mut self) -> Option<ServiceWorkerOutput<TC>> {
        todo!()
    }
}
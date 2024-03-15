pub const SERVICE_ID: u8 = 0;
pub const SERVICE_NAME: &str = "router_sync";

use super::{Service, ServiceWorker, ServiceWorkerInput, ServiceWorkerOutput};

pub struct RouterSyncService {

}

impl<TC, TW> Service<TC, TW> for RouterSyncService {
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

pub struct RouterSyncServiceWorker {

}

impl<TC, TW> ServiceWorker<TC, TW> for RouterSyncServiceWorker {
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
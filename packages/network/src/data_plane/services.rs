use std::marker::PhantomData;
use std::sync::Arc;

use crate::base::{ServiceBuilder, ServiceId, ServiceWorker, ServiceWorkerInput, ServiceWorkerOutput};
use crate::features::{FeaturesControl, FeaturesEvent};
use crate::san_io_utils::TasksSwitcher;

/// To manage the services we need to create an object that will hold the services
pub struct ServiceWorkerManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    services: [Option<Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, ServiceEvent, ToController, ToWorker>>>; 256],
    switcher: TasksSwitcher<256>,
    last_input_service: Option<ServiceId>,
    _tmp: PhantomData<ServiceControl>,
}

impl<ServiceControl, ServiceEvent, ToController, ToWorker> ServiceWorkerManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        Self {
            services: std::array::from_fn(|index| services.iter().find(|s| s.service_id() == index as u8).map(|s| s.create_worker())),
            switcher: TasksSwitcher::default(),
            last_input_service: None,
            _tmp: PhantomData,
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        for service in self.services.iter_mut() {
            if let Some(service) = service {
                service.on_tick(now);
            }
        }
    }

    pub fn on_input(&mut self, now: u64, id: ServiceId, input: ServiceWorkerInput<FeaturesEvent, ToWorker>) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ServiceEvent, ToController>> {
        let service = self.services[*id as usize].as_mut()?;
        self.last_input_service = Some(id);
        service.on_input(now, input)
    }

    pub fn pop_output(&mut self) -> Option<(ServiceId, ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ServiceEvent, ToController>)> {
        if let Some(last_service) = self.last_input_service {
            let res = self.services[*last_service as usize].as_mut().map(|s| s.pop_output()).flatten();
            if res.is_none() {
                self.last_input_service = None;
            }

            res.map(|o| (last_service, o))
        } else {
            loop {
                let s = &mut self.switcher;
                let index = s.current()?;
                if let Some(Some(service)) = self.services.get_mut(index) {
                    if let Some(output) = s.process(service.pop_output()) {
                        return Some(((index as u8).into(), output));
                    }
                } else {
                    s.process::<()>(None);
                }
            }
        }
    }
}

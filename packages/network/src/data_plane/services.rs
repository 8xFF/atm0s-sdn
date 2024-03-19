use crate::base::{ServiceId, ServiceWorker, ServiceWorkerInput, ServiceWorkerOutput};
use crate::features::{FeaturesControl, FeaturesEvent};
use crate::san_io_utils::TasksSwitcher;

/// To manage the services we need to create an object that will hold the services
pub struct ServiceWorkerManager<ToController, ToWorker> {
    services: [Option<Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, ToController, ToWorker>>>; 256],
    switcher: TasksSwitcher<256>,
    last_input_service: Option<ServiceId>,
}

impl<ToController, ToWorker> ServiceWorkerManager<ToController, ToWorker> {
    pub fn new() -> Self {
        Self {
            services: std::array::from_fn(|_| None),
            switcher: TasksSwitcher::default(),
            last_input_service: None,
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        for service in self.services.iter_mut() {
            if let Some(service) = service {
                service.on_tick(now);
            }
        }
    }

    pub fn on_input(&mut self, now: u64, id: ServiceId, input: ServiceWorkerInput<FeaturesEvent, ToWorker>) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ToController>> {
        let service = self.services[*id as usize].as_mut()?;
        self.last_input_service = Some(id);
        service.on_input(now, input)
    }

    pub fn pop_output(&mut self) -> Option<(ServiceId, ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ToController>)> {
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

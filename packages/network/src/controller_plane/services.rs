use crate::base::{ServiceId, ServiceInput, ServiceOutput, ServiceSharedInput};
use crate::features::{FeaturesControl, FeaturesEvent};
use crate::{base::Service, san_io_utils::TasksSwitcher};

/// To manage the services we need to create an object that will hold the services
pub struct ServiceManager<ToController, ToWorker> {
    services: [Option<Box<dyn Service<FeaturesControl, FeaturesEvent, ToController, ToWorker>>>; 256],
    switcher: TasksSwitcher<256>,
    last_input_service: Option<ServiceId>,
}

impl<ToController, ToWorker> ServiceManager<ToController, ToWorker> {
    pub fn new() -> Self {
        Self {
            services: std::array::from_fn(|_| None),
            switcher: TasksSwitcher::default(),
            last_input_service: None,
        }
    }

    pub fn on_shared_input<'a>(&mut self, now: u64, input: ServiceSharedInput<'a>) {
        self.last_input_service = None;
        for service in self.services.iter_mut() {
            if let Some(service) = service {
                service.on_shared_input(now, input.clone());
            }
        }
    }

    pub fn on_input(&mut self, now: u64, id: ServiceId, input: ServiceInput<FeaturesEvent, ToController>) {
        if let Some(service) = self.services.get_mut(*id as usize) {
            if let Some(service) = service {
                self.last_input_service = Some(id);
                service.on_input(now, input);
            }
        }
    }

    pub fn pop_output(&mut self) -> Option<(ServiceId, ServiceOutput<FeaturesControl, ToWorker>)> {
        if let Some(last_service) = self.last_input_service {
            let out = self.services[*last_service as usize].as_mut()?.pop_output()?;
            Some((last_service, out))
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

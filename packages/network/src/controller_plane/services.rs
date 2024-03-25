use std::sync::Arc;

use crate::base::{ServiceBuilder, ServiceCtx, ServiceId, ServiceInput, ServiceOutput, ServiceSharedInput};
use crate::features::{FeaturesControl, FeaturesEvent};
use crate::{base::Service, san_io_utils::TasksSwitcher};

/// To manage the services we need to create an object that will hold the services
pub struct ServiceManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    services: [Option<Box<dyn Service<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>; 256],
    switcher: TasksSwitcher<u8, 256>,
}

impl<ServiceControl, ServiceEvent, ToController, ToWorker> ServiceManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        Self {
            services: std::array::from_fn(|index| services.iter().find(|s| s.service_id() == index as u8).map(|s| s.create())),
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_shared_input(&mut self, ctx: &ServiceCtx, now: u64, input: ServiceSharedInput) {
        self.switcher.push_all();
        for service in self.services.iter_mut() {
            if let Some(service) = service {
                service.on_shared_input(ctx, now, input.clone());
            }
        }
    }

    pub fn on_input(&mut self, ctx: &ServiceCtx, now: u64, id: ServiceId, input: ServiceInput<FeaturesEvent, ServiceControl, ToController>) {
        if let Some(service) = self.services.get_mut(*id as usize) {
            if let Some(service) = service {
                self.switcher.push_last(*id);
                service.on_input(ctx, now, input);
            }
        }
    }

    pub fn pop_output(&mut self, ctx: &ServiceCtx) -> Option<(ServiceId, ServiceOutput<FeaturesControl, ServiceEvent, ToWorker>)> {
        loop {
            let s = &mut self.switcher;
            let index = s.current()?;
            if let Some(Some(service)) = self.services.get_mut(index as usize) {
                if let Some(output) = s.process(service.pop_output(ctx)) {
                    return Some(((index as u8).into(), output));
                }
            } else {
                s.process::<()>(None);
            }
        }
    }
}

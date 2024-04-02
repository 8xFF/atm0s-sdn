use std::sync::Arc;

use sans_io_runtime::TaskSwitcher;

use crate::base::Service;
use crate::base::{ServiceBuilder, ServiceCtx, ServiceId, ServiceInput, ServiceOutput, ServiceSharedInput};
use crate::features::{FeaturesControl, FeaturesEvent};

/// To manage the services we need to create an object that will hold the services
pub struct ServiceManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    services: [Option<Box<dyn Service<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>; 256],
    switcher: TaskSwitcher,
}

impl<ServiceControl, ServiceEvent, ToController, ToWorker> ServiceManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        let max_service_id = services.iter().map(|s| s.service_id()).max().unwrap_or(0);
        Self {
            services: std::array::from_fn(|index| services.iter().find(|s| s.service_id() == index as u8).map(|s| s.create())),
            switcher: TaskSwitcher::new(max_service_id as usize + 1),
        }
    }

    pub fn on_shared_input(&mut self, ctx: &ServiceCtx, now: u64, input: ServiceSharedInput) {
        for service in self.services.iter_mut() {
            if let Some(service) = service {
                self.switcher.queue_flag_task(service.service_id() as usize);
                service.on_shared_input(ctx, now, input.clone());
            }
        }
    }

    pub fn on_input(&mut self, ctx: &ServiceCtx, now: u64, id: ServiceId, input: ServiceInput<FeaturesEvent, ServiceControl, ToController>) {
        if let Some(service) = self.services.get_mut(*id as usize) {
            if let Some(service) = service {
                self.switcher.queue_flag_task(*id as usize);
                service.on_input(ctx, now, input);
            }
        }
    }

    pub fn pop_output(&mut self, ctx: &ServiceCtx) -> Option<(ServiceId, ServiceOutput<FeaturesControl, ServiceEvent, ToWorker>)> {
        loop {
            let s = &mut self.switcher;
            let index = s.queue_current()? as u8;
            if let Some(Some(service)) = self.services.get_mut(index as usize) {
                if let Some(output) = s.queue_process(service.pop_output(ctx)) {
                    return Some(((index as u8).into(), output));
                }
            } else {
                s.queue_process::<()>(None);
            }
        }
    }
}

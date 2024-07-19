use std::sync::Arc;

use sans_io_runtime::{TaskSwitcher, TaskSwitcherBranch, TaskSwitcherChild};

use crate::base::Service;
use crate::base::{ServiceBuilder, ServiceCtx, ServiceId, ServiceInput, ServiceOutput, ServiceSharedInput};
use crate::features::{FeaturesControl, FeaturesEvent};

pub type Output<UserData, ServiceEvent, ToWorker> = (ServiceId, ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>);

/// To manage the services we need to create an object that will hold the services

pub struct ServiceManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    services: [Option<
        TaskSwitcherBranch<
            Box<dyn Service<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>,
            ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>,
        >,
    >; 256],
    switcher: TaskSwitcher,
}

impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> ServiceManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        let max_service_id = services.iter().map(|s| s.service_id()).max().unwrap_or(0);
        Self {
            services: std::array::from_fn(|index| services.iter().find(|s| s.service_id() == index as u8).map(|s| TaskSwitcherBranch::new(s.create(), index))),
            switcher: TaskSwitcher::new(max_service_id as usize + 1),
        }
    }

    pub fn on_shared_input(&mut self, ctx: &ServiceCtx, now: u64, input: ServiceSharedInput) {
        for service in self.services.iter_mut() {
            if let Some(service) = service {
                service.input(&mut self.switcher).on_shared_input(ctx, now, input.clone());
            }
        }
    }

    pub fn on_input(&mut self, ctx: &ServiceCtx, now: u64, id: ServiceId, input: ServiceInput<UserData, FeaturesEvent, ServiceControl, ToController>) {
        if let Some(service) = self.services.get_mut(*id as usize) {
            if let Some(service) = service {
                self.switcher.flag_task(*id as usize);
                service.input(&mut self.switcher).on_input(ctx, now, input);
            }
        }
    }
}

impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> TaskSwitcherChild<(ServiceId, ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>)>
    for ServiceManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>
{
    type Time = u64;
    fn pop_output(&mut self, now: u64) -> Option<(ServiceId, ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>)> {
        loop {
            let index = self.switcher.current()?;
            if let Some(Some(service)) = self.services.get_mut(index) {
                if let Some(output) = service.pop_output(now, &mut self.switcher) {
                    return Some(((index as u8).into(), output));
                }
            } else {
                self.switcher.finished(index);
            }
        }
    }
}

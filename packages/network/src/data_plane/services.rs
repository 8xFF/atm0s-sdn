use std::marker::PhantomData;
use std::sync::Arc;

use sans_io_runtime::{TaskSwitcher, TaskSwitcherBranch, TaskSwitcherChild};

use crate::base::{ServiceBuilder, ServiceId, ServiceWorker, ServiceWorkerCtx, ServiceWorkerInput, ServiceWorkerOutput};
use crate::features::{FeaturesControl, FeaturesEvent};

pub type Output<UserData, ServiceControl, ServiceEvent, ToController> = (ServiceId, ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController>);

/// To manage the services we need to create an object that will hold the services
#[allow(clippy::type_complexity)]
pub struct ServiceWorkerManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    services: [Option<
        TaskSwitcherBranch<
            Box<dyn ServiceWorker<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>,
            ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController>,
        >,
    >; 256],
    switcher: TaskSwitcher,
    _tmp: PhantomData<ServiceControl>,
}

#[allow(clippy::type_complexity)]
impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> ServiceWorkerManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        let max_service_id = services.iter().map(|s| s.service_id()).max().unwrap_or(0);
        Self {
            services: std::array::from_fn(|index| services.iter().find(|s| s.service_id() == index as u8).map(|s| TaskSwitcherBranch::new(s.create_worker(), index))),
            switcher: TaskSwitcher::new(max_service_id as usize + 1),
            _tmp: PhantomData,
        }
    }

    pub fn on_tick(&mut self, ctx: &ServiceWorkerCtx, now: u64, tick_count: u64) {
        for service in self.services.iter_mut().flatten() {
            service.input(&mut self.switcher).on_tick(ctx, now, tick_count);
        }
    }

    pub fn on_input(&mut self, ctx: &ServiceWorkerCtx, now: u64, id: ServiceId, input: ServiceWorkerInput<UserData, FeaturesEvent, ServiceControl, ToWorker>) {
        if let Some(service) = self.services[*id as usize].as_mut() {
            self.switcher.flag_task(*id as usize);
            service.input(&mut self.switcher).on_input(ctx, now, input);
        }
    }
}

impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> TaskSwitcherChild<Output<UserData, ServiceControl, ServiceEvent, ToController>>
    for ServiceWorkerManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>
{
    type Time = u64;
    fn pop_output(&mut self, now: u64) -> Option<Output<UserData, ServiceControl, ServiceEvent, ToController>> {
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

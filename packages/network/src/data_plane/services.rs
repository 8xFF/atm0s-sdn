use std::marker::PhantomData;
use std::sync::Arc;

use sans_io_runtime::TaskSwitcher;

use crate::base::{ServiceBuilder, ServiceId, ServiceWorker, ServiceWorkerCtx, ServiceWorkerInput, ServiceWorkerOutput};
use crate::features::{FeaturesControl, FeaturesEvent};

/// To manage the services we need to create an object that will hold the services
pub struct ServiceWorkerManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    services: [Option<Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, ServiceEvent, ToController, ToWorker>>>; 256],
    switcher: TaskSwitcher,
    _tmp: PhantomData<ServiceControl>,
}

impl<ServiceControl, ServiceEvent, ToController, ToWorker> ServiceWorkerManager<ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        let max_service_id = services.iter().map(|s| s.service_id()).max().unwrap_or(0);
        Self {
            services: std::array::from_fn(|index| services.iter().find(|s| s.service_id() == index as u8).map(|s| s.create_worker())),
            switcher: TaskSwitcher::new(max_service_id as usize + 1),
            _tmp: PhantomData,
        }
    }

    pub fn on_tick(&mut self, ctx: &ServiceWorkerCtx, now: u64, tick_count: u64) {
        for service in self.services.iter_mut() {
            if let Some(service) = service {
                self.switcher.queue_flag_task(service.service_id() as usize);
                service.on_tick(ctx, now, tick_count);
            }
        }
    }

    pub fn on_input(
        &mut self,
        ctx: &ServiceWorkerCtx,
        now: u64,
        id: ServiceId,
        input: ServiceWorkerInput<FeaturesEvent, ToWorker>,
    ) -> Option<ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ServiceEvent, ToController>> {
        let service = self.services[*id as usize].as_mut()?;
        let out = service.on_input(ctx, now, input);
        if out.is_some() {
            self.switcher.queue_flag_task(*id as usize);
        }
        out
    }

    pub fn pop_output(&mut self, ctx: &ServiceWorkerCtx) -> Option<(ServiceId, ServiceWorkerOutput<FeaturesControl, FeaturesEvent, ServiceEvent, ToController>)> {
        loop {
            let s = &mut self.switcher;
            let index = s.queue_current()?;
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

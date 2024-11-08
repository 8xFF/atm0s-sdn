use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::Arc;

use sans_io_runtime::{TaskSwitcher, TaskSwitcherBranch, TaskSwitcherChild};

use crate::base::{ServiceBuilder, ServiceId, ServiceWorker, ServiceWorkerCtx, ServiceWorkerInput, ServiceWorkerOutput};
use crate::features::{FeaturesControl, FeaturesEvent};

pub enum Output<UserData, ServiceControl, ServiceEvent, ToController> {
    Output(ServiceId, ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController>),
    OnResourceEmpty,
}

struct ServiceSlot<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    service: TaskSwitcherBranch<
        Box<dyn ServiceWorker<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>,
        ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController>,
    >,
    is_empty: bool,
}

/// To manage the services we need to create an object that will hold the services
#[allow(clippy::type_complexity)]
pub struct ServiceWorkerManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    services: [Option<ServiceSlot<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>>; 256],
    services_count: usize,
    switcher: TaskSwitcher,
    empty_services: HashSet<ServiceId>,
    shutdown: bool,
    _tmp: PhantomData<ServiceControl>,
}

#[allow(clippy::type_complexity)]
impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> ServiceWorkerManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        let max_service_id = services.iter().map(|s| s.service_id()).max().unwrap_or(0);
        Self {
            services_count: services.len(),
            services: std::array::from_fn(|index| {
                services.iter().find(|s| s.service_id() == index as u8).map(|s| ServiceSlot {
                    service: TaskSwitcherBranch::new(s.create_worker(), index),
                    is_empty: true,
                })
            }),
            switcher: TaskSwitcher::new(max_service_id as usize + 1),
            empty_services: HashSet::new(),
            shutdown: false,
            _tmp: PhantomData,
        }
    }

    pub fn on_tick(&mut self, ctx: &ServiceWorkerCtx, now: u64, tick_count: u64) {
        for service in self.services.iter_mut().flatten() {
            service.service.input(&mut self.switcher).on_tick(ctx, now, tick_count);
        }
    }

    pub fn on_input(&mut self, ctx: &ServiceWorkerCtx, now: u64, id: ServiceId, input: ServiceWorkerInput<UserData, FeaturesEvent, ServiceControl, ToWorker>) {
        if let Some(service) = self.services[*id as usize].as_mut() {
            self.switcher.flag_task(*id as usize);
            service.service.input(&mut self.switcher).on_input(ctx, now, input);
        }
    }

    pub fn on_shutdown(&mut self, ctx: &ServiceWorkerCtx, now: u64) {
        if self.shutdown {
            return;
        }
        for service in self.services.iter_mut().flatten() {
            service.service.input(&mut self.switcher).on_shutdown(ctx, now);
        }
        self.shutdown = true;
    }
}

impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> TaskSwitcherChild<Output<UserData, ServiceControl, ServiceEvent, ToController>>
    for ServiceWorkerManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>
{
    type Time = u64;

    fn empty_event(&self) -> Output<UserData, ServiceControl, ServiceEvent, ToController> {
        Output::OnResourceEmpty
    }

    fn is_empty(&self) -> bool {
        self.shutdown && self.empty_services.len() == self.services_count
    }

    fn pop_output(&mut self, now: u64) -> Option<Output<UserData, ServiceControl, ServiceEvent, ToController>> {
        loop {
            let index = self.switcher.current()?;
            if let Some(Some(slot)) = self.services.get_mut(index) {
                if let Some(output) = slot.service.pop_output(now, &mut self.switcher) {
                    return Some(Output::Output((index as u8).into(), output));
                } else {
                    if !slot.is_empty {
                        if slot.service.is_empty() {
                            slot.is_empty = true;
                            self.empty_services.insert((index as u8).into());
                            return Some(Output::Output((index as u8).into(), slot.service.empty_event()));
                        }
                    } else {
                        #[allow(clippy::collapsible_else_if)]
                        if !slot.service.is_empty() {
                            slot.is_empty = false;
                            self.empty_services.remove(&(index as u8).into());
                        }
                    }
                    self.switcher.finished(index);
                }
            } else {
                self.switcher.finished(index);
            }
        }
    }
}

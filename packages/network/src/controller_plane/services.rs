use std::collections::HashSet;
use std::sync::Arc;

use sans_io_runtime::{TaskSwitcher, TaskSwitcherBranch, TaskSwitcherChild};

use crate::base::Service;
use crate::base::{ServiceBuilder, ServiceCtx, ServiceId, ServiceInput, ServiceOutput, ServiceSharedInput};
use crate::features::{FeaturesControl, FeaturesEvent};

pub enum Output<UserData, ServiceEvent, ToWorker> {
    Output(ServiceId, ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>),
    OnResourceEmpty,
}

type ServiceBox<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> = Box<dyn Service<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>;
type ServiceBoxOutput<UserData, ServiceEvent, ToWorker> = ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>;
type ServiceSwitcher<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> =
    TaskSwitcherBranch<ServiceBox<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>, ServiceBoxOutput<UserData, ServiceEvent, ToWorker>>;

struct ServiceSlot<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    service: ServiceSwitcher<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>,
    is_empty: bool,
}

/// To manage the services we need to create an object that will hold the services
pub struct ServiceManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    #[allow(clippy::type_complexity)]
    services: [Option<ServiceSlot<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>>; 256],
    services_count: usize,
    empty_services: HashSet<ServiceId>,
    switcher: TaskSwitcher,
    shutdown: bool,
}

#[allow(clippy::type_complexity)]
impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> ServiceManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> {
    pub fn new(services: Vec<Arc<dyn ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>>) -> Self {
        let max_service_id = services.iter().map(|s| s.service_id()).max().unwrap_or(0);
        Self {
            services_count: services.len(),
            services: std::array::from_fn(|index| {
                services.iter().find(|s| s.service_id() == index as u8).map(|s| ServiceSlot {
                    service: TaskSwitcherBranch::new(s.create(), index),
                    is_empty: false,
                })
            }),
            empty_services: HashSet::default(),
            switcher: TaskSwitcher::new(max_service_id as usize + 1),
            shutdown: false,
        }
    }

    pub fn on_shared_input(&mut self, ctx: &ServiceCtx, now: u64, input: ServiceSharedInput) {
        for service in self.services.iter_mut().flatten() {
            service.service.input(&mut self.switcher).on_shared_input(ctx, now, input.clone());
        }
    }

    pub fn on_input(&mut self, ctx: &ServiceCtx, now: u64, id: ServiceId, input: ServiceInput<UserData, FeaturesEvent, ServiceControl, ToController>) {
        if let Some(Some(service)) = self.services.get_mut(*id as usize) {
            self.switcher.flag_task(*id as usize);
            service.service.input(&mut self.switcher).on_input(ctx, now, input);
        }
    }

    pub fn on_shutdown(&mut self, ctx: &ServiceCtx, now: u64) {
        if self.shutdown {
            return;
        }
        log::info!("[ControllerPlane] Services Shutdown");
        for service in self.services.iter_mut().flatten() {
            service.service.input(&mut self.switcher).on_shutdown(ctx, now);
        }
        self.shutdown = true;
    }
}

impl<UserData, ServiceControl, ServiceEvent, ToController, ToWorker> TaskSwitcherChild<Output<UserData, ServiceEvent, ToWorker>>
    for ServiceManager<UserData, ServiceControl, ServiceEvent, ToController, ToWorker>
{
    type Time = u64;

    fn empty_event(&self) -> Output<UserData, ServiceEvent, ToWorker> {
        Output::OnResourceEmpty
    }

    fn is_empty(&self) -> bool {
        self.shutdown && self.empty_services.len() == self.services_count
    }

    fn pop_output(&mut self, now: u64) -> Option<Output<UserData, ServiceEvent, ToWorker>> {
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

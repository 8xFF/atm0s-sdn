use atm0s_sdn_identity::NodeId;
use atm0s_sdn_utils::simple_pub_type;
use sans_io_runtime::TaskSwitcherChild;

use super::ConnectionEvent;

simple_pub_type!(ServiceId, u8);

/// First part is Service, which is running inside the controller.

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum ServiceControlActor<UserData> {
    Controller(UserData),
    Worker(u16, UserData),
}

#[derive(Debug, Clone)]
pub enum ServiceSharedInput {
    Tick(u64),
    Connection(ConnectionEvent),
}

#[derive(Debug)]
pub enum ServiceInput<UserData, FeaturesEvent, ServiceControl, ToController> {
    Control(ServiceControlActor<UserData>, ServiceControl),
    FromWorker(ToController),
    FeatureEvent(FeaturesEvent),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker> {
    Event(ServiceControlActor<UserData>, ServiceEvent),
    FeatureControl(FeaturesControl),
    BroadcastWorkers(ToWorker),
}

pub struct ServiceCtx {
    pub node_id: NodeId,
    pub session: u64,
}

pub trait Service<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker> {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;
    fn on_shared_input(&mut self, _ctx: &ServiceCtx, _now: u64, _input: ServiceSharedInput);
    fn on_input(&mut self, _ctx: &ServiceCtx, _now: u64, input: ServiceInput<UserData, FeaturesEvent, ServiceControl, ToController>);
    fn pop_output2(&mut self, _now: u64) -> Option<ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>>;
}

impl<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker> TaskSwitcherChild<ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>>
    for Box<dyn Service<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>
{
    type Time = u64;
    fn pop_output(&mut self, now: u64) -> Option<ServiceOutput<UserData, FeaturesControl, ServiceEvent, ToWorker>> {
        self.pop_output2(now)
    }
}

/// Second part is Worker, which is running inside each data plane workers.

pub enum ServiceWorkerInput<UserData, FeaturesEvent, ServiceControl, ToWorker> {
    Control(ServiceControlActor<UserData>, ServiceControl),
    FromController(ToWorker),
    FeatureEvent(FeaturesEvent),
}

pub enum ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController> {
    ForwardControlToController(ServiceControlActor<UserData>, ServiceControl),
    ForwardFeatureEventToController(FeaturesEvent),
    ToController(ToController),
    FeatureControl(FeaturesControl),
    Event(ServiceControlActor<UserData>, ServiceEvent),
}

pub struct ServiceWorkerCtx {
    pub node_id: NodeId,
}

pub trait ServiceWorker<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker> {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;
    fn on_tick(&mut self, _ctx: &ServiceWorkerCtx, _now: u64, _tick_count: u64);
    fn on_input(&mut self, _ctx: &ServiceWorkerCtx, _now: u64, input: ServiceWorkerInput<UserData, FeaturesEvent, ServiceControl, ToWorker>);
    fn pop_output2(&mut self, _now: u64) -> Option<ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController>>;
}

impl<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>
    TaskSwitcherChild<ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController>>
    for Box<dyn ServiceWorker<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>
{
    type Time = u64;
    fn pop_output(&mut self, now: u64) -> Option<ServiceWorkerOutput<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController>> {
        self.pop_output2(now)
    }
}

pub trait ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>: Send + Sync {
    fn service_id(&self) -> u8;
    fn service_name(&self) -> &str;
    fn discoverable(&self) -> bool {
        true
    }
    fn create(&self) -> Box<dyn Service<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>;
    fn create_worker(&self) -> Box<dyn ServiceWorker<UserData, FeaturesControl, FeaturesEvent, ServiceControl, ServiceEvent, ToController, ToWorker>>;
}

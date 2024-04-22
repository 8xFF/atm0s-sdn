use std::{collections::VecDeque, fmt::Debug, hash::Hash, sync::Arc};

use atm0s_sdn_identity::NodeId;
use rand::RngCore;
use sans_io_runtime::TaskSwitcher;

use crate::{
    base::{
        Authorization, ConnectionEvent, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, HandshakeBuilder, ServiceBuilder, ServiceControlActor, ServiceCtx,
        ServiceInput, ServiceOutput, ServiceSharedInput,
    },
    features::{FeaturesControl, FeaturesEvent},
    ExtIn, ExtOut, LogicControl, LogicEvent,
};

use self::{features::FeatureManager, neighbours::NeighboursManager, services::ServiceManager};

mod features;
mod neighbours;
mod services;

#[derive(Debug, Clone, convert_enum::From)]
pub enum Input<UserData, SC, SE, TC> {
    Ext(ExtIn<UserData, SC>),
    Control(LogicControl<UserData, SC, SE, TC>),
    #[convert_enum(optout)]
    ShutdownRequest,
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum Output<UserData, SE, TW> {
    Ext(ExtOut<UserData, SE>),
    Event(LogicEvent<UserData, SE, TW>),
    #[convert_enum(optout)]
    ShutdownSuccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive, num_enum::IntoPrimitive)]
#[repr(usize)]
enum TaskType {
    Neighbours = 0,
    Feature = 1,
    Service = 2,
}

pub struct ControllerPlaneCfg<UserData, SC, SE, TC, TW> {
    pub session: u64,
    pub services: Vec<Arc<dyn ServiceBuilder<UserData, FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>,
    pub authorization: Arc<dyn Authorization>,
    pub handshake_builder: Arc<dyn HandshakeBuilder>,
    pub random: Box<dyn RngCore + Send + Sync>,
}

pub struct ControllerPlane<UserData, SC, SE, TC, TW> {
    tick_count: u64,
    neighbours: NeighboursManager,
    feature_ctx: FeatureContext,
    features: FeatureManager<UserData>,
    service_ctx: ServiceCtx,
    services: ServiceManager<UserData, SC, SE, TC, TW>,
    switcher: TaskSwitcher,
    queue: VecDeque<Output<UserData, SE, TW>>,
}

impl<UserData, SC, SE, TC, TW> ControllerPlane<UserData, SC, SE, TC, TW>
where
    UserData: 'static + Hash + Copy + Eq + Debug,
{
    /// Create a new ControllerPlane
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node id of the current node
    /// * `session` - The session id of the current node, it can be a random number
    ///
    /// # Returns
    ///
    /// A new ControllerPlane
    pub fn new(node_id: NodeId, cfg: ControllerPlaneCfg<UserData, SC, SE, TC, TW>) -> Self {
        log::info!("Create ControllerPlane for node: {}, running session {}", node_id, cfg.session);
        let service_ids = cfg.services.iter().filter(|s| s.discoverable()).map(|s| s.service_id()).collect();

        Self {
            tick_count: 0,
            neighbours: NeighboursManager::new(node_id, cfg.authorization, cfg.handshake_builder, cfg.random),
            feature_ctx: FeatureContext { node_id, session: cfg.session },
            features: FeatureManager::new(node_id, cfg.session, service_ids),
            service_ctx: ServiceCtx { node_id, session: cfg.session },
            services: ServiceManager::new(cfg.services),
            switcher: TaskSwitcher::new(3), //3 types: Neighbours, Feature, Service
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        log::trace!("[ControllerPlane] on_tick: {}", now_ms);
        self.switcher.queue_flag_all();
        self.neighbours.on_tick(now_ms, self.tick_count);
        self.features.on_shared_input(&self.feature_ctx, now_ms, FeatureSharedInput::Tick(self.tick_count));
        self.services.on_shared_input(&self.service_ctx, now_ms, ServiceSharedInput::Tick(self.tick_count));
        self.tick_count += 1;
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input<UserData, SC, SE, TC>) {
        match event {
            Input::Ext(ExtIn::ConnectTo(addr)) => {
                self.switcher.queue_flag_task(TaskType::Neighbours as usize);
                self.neighbours.on_input(now_ms, neighbours::Input::ConnectTo(addr));
            }
            Input::Ext(ExtIn::DisconnectFrom(node)) => {
                self.switcher.queue_flag_task(TaskType::Neighbours as usize);
                self.neighbours.on_input(now_ms, neighbours::Input::DisconnectFrom(node));
            }
            Input::Ext(ExtIn::FeaturesControl(userdata, control)) => {
                self.switcher.queue_flag_task(TaskType::Feature as usize);
                self.features.on_input(
                    &self.feature_ctx,
                    now_ms,
                    control.to_feature(),
                    FeatureInput::Control(FeatureControlActor::Controller(userdata), control),
                );
            }
            Input::Ext(ExtIn::ServicesControl(service, userdata, control)) => {
                self.switcher.queue_flag_task(TaskType::Service as usize);
                self.services
                    .on_input(&self.service_ctx, now_ms, service, ServiceInput::Control(ServiceControlActor::Controller(userdata), control));
            }
            Input::Control(LogicControl::NetNeighbour(remote, control)) => {
                self.switcher.queue_flag_task(TaskType::Neighbours as usize);
                self.neighbours.on_input(now_ms, neighbours::Input::Control(remote, control));
            }
            Input::Control(LogicControl::Feature(to)) => {
                self.switcher.queue_flag_task(TaskType::Feature as usize);
                self.features.on_input(&self.feature_ctx, now_ms, to.to_feature(), FeatureInput::FromWorker(to));
            }
            Input::Control(LogicControl::Service(service, to)) => {
                self.switcher.queue_flag_task(TaskType::Service as usize);
                self.services.on_input(&self.service_ctx, now_ms, service, ServiceInput::FromWorker(to));
            }
            Input::Control(LogicControl::NetRemote(feature, conn, meta, msg)) => {
                if let Some(ctx) = self.neighbours.conn(conn) {
                    self.switcher.queue_flag_task(TaskType::Feature as usize);
                    self.features.on_input(&self.feature_ctx, now_ms, feature, FeatureInput::Net(ctx, meta, msg));
                }
            }
            Input::Control(LogicControl::NetLocal(feature, meta, msg)) => {
                self.switcher.queue_flag_task(TaskType::Feature as usize);
                self.features.on_input(&self.feature_ctx, now_ms, feature, FeatureInput::Local(meta, msg));
            }
            Input::Control(LogicControl::ServiceEvent(service, event)) => {
                self.switcher.queue_flag_task(TaskType::Service as usize);
                self.services.on_input(&self.service_ctx, now_ms, service, ServiceInput::FeatureEvent(event));
            }
            Input::Control(LogicControl::ServicesControl(actor, service, control)) => {
                self.switcher.queue_flag_task(TaskType::Service as usize);
                self.services.on_input(&self.service_ctx, now_ms, service, ServiceInput::Control(actor, control));
            }
            Input::Control(LogicControl::FeaturesControl(actor, control)) => {
                self.switcher.queue_flag_task(TaskType::Feature as usize);
                self.features.on_input(&self.feature_ctx, now_ms, control.to_feature(), FeatureInput::Control(actor, control));
            }
            Input::Control(LogicControl::ExtFeaturesEvent(userdata, event)) => {
                self.queue.push_back(Output::Ext(ExtOut::FeaturesEvent(userdata, event)));
            }
            Input::Control(LogicControl::ExtServicesEvent(service, userdata, event)) => {
                self.queue.push_back(Output::Ext(ExtOut::ServicesEvent(service, userdata, event)));
            }
            Input::ShutdownRequest => {
                self.switcher.queue_flag_task(TaskType::Neighbours as usize);
                self.neighbours.on_input(now_ms, neighbours::Input::ShutdownRequest);
            }
        }
    }

    pub fn pop_output(&mut self, now_ms: u64) -> Option<Output<UserData, SE, TW>> {
        while let Some(out) = self.queue.pop_front() {
            return Some(out);
        }

        while let Some(current) = self.switcher.queue_current() {
            match current.try_into().expect("Should convert to TaskType") {
                TaskType::Neighbours => {
                    let out = self.pop_neighbours(now_ms);
                    if let Some(out) = self.switcher.queue_process(out) {
                        return Some(out);
                    }
                }
                TaskType::Feature => {
                    let out = self.pop_features(now_ms);
                    if let Some(out) = self.switcher.queue_process(out) {
                        return Some(out);
                    }
                }
                TaskType::Service => {
                    let out = self.pop_services(now_ms);
                    if let Some(out) = self.switcher.queue_process(out) {
                        return Some(out);
                    }
                }
            }
        }

        None
    }

    fn pop_neighbours(&mut self, now_ms: u64) -> Option<Output<UserData, SE, TW>> {
        loop {
            let out = self.neighbours.pop_output(now_ms)?;
            let out = match out {
                neighbours::Output::Control(remote, control) => Some(Output::Event(LogicEvent::NetNeighbour(remote, control))),
                neighbours::Output::Event(event) => {
                    self.switcher.queue_flag_task(TaskType::Feature as usize);
                    self.features.on_shared_input(&self.feature_ctx, now_ms, FeatureSharedInput::Connection(event.clone()));
                    self.switcher.queue_flag_task(TaskType::Service as usize);
                    self.services.on_shared_input(&self.service_ctx, now_ms, ServiceSharedInput::Connection(event.clone()));
                    match event {
                        ConnectionEvent::Connected(ctx, secure) => Some(Output::Event(LogicEvent::Pin(ctx.conn, ctx.node, ctx.remote, secure))),
                        ConnectionEvent::Stats(_ctx, _stats) => None,
                        ConnectionEvent::Disconnected(ctx) => Some(Output::Event(LogicEvent::UnPin(ctx.conn))),
                    }
                }
                neighbours::Output::ShutdownResponse => Some(Output::ShutdownSuccess),
            };
            if out.is_some() {
                return out;
            }
        }
    }

    fn pop_features(&mut self, now_ms: u64) -> Option<Output<UserData, SE, TW>> {
        let (feature, out) = self.features.pop_output(&self.feature_ctx)?;
        match out {
            FeatureOutput::ToWorker(is_broadcast, to) => Some(Output::Event(LogicEvent::Feature(is_broadcast, to))),
            FeatureOutput::Event(actor, event) => {
                log::debug!("[Controller] send FeatureEvent to actor {:?}, event {:?}", actor, event);
                match actor {
                    FeatureControlActor::Controller(userdata) => Some(Output::Ext(ExtOut::FeaturesEvent(userdata, event))),
                    FeatureControlActor::Worker(worker, userdata) => Some(Output::Event(LogicEvent::ExtFeaturesEvent(worker, userdata, event))),
                    FeatureControlActor::Service(service) => {
                        self.switcher.queue_flag_task(TaskType::Service as usize);
                        self.services.on_input(&self.service_ctx, now_ms, service, ServiceInput::FeatureEvent(event));
                        self.pop_services(now_ms)
                    }
                }
            }
            FeatureOutput::SendDirect(conn, meta, buf) => {
                log::debug!("[ControllerPlane] SendDirect to conn: {:?}, len: {}", conn, buf.len());
                Some(Output::Event(LogicEvent::NetDirect(feature, self.neighbours.conn(conn)?.remote, conn, meta, buf)))
            }
            FeatureOutput::SendRoute(rule, ttl, buf) => {
                log::debug!("[ControllerPlane] SendRoute to rule: {:?}, len: {}", rule, buf.len());
                Some(Output::Event(LogicEvent::NetRoute(feature, rule, ttl, buf)))
            }
            FeatureOutput::NeighboursConnectTo(addr) => {
                self.switcher.queue_flag_task(TaskType::Neighbours as usize);
                self.neighbours.on_input(now_ms, neighbours::Input::ConnectTo(addr));
                self.pop_neighbours(now_ms)
            }
            FeatureOutput::NeighboursDisconnectFrom(node) => {
                self.switcher.queue_flag_task(TaskType::Neighbours as usize);
                self.neighbours.on_input(now_ms, neighbours::Input::DisconnectFrom(node));
                self.pop_neighbours(now_ms)
            }
        }
    }

    fn pop_services(&mut self, now_ms: u64) -> Option<Output<UserData, SE, TW>> {
        let (service, out) = self.services.pop_output(&self.service_ctx)?;
        match out {
            ServiceOutput::FeatureControl(control) => {
                self.switcher.queue_flag_task(TaskType::Feature as usize);
                self.features
                    .on_input(&self.feature_ctx, now_ms, control.to_feature(), FeatureInput::Control(FeatureControlActor::Service(service), control));
                self.pop_features(now_ms)
            }
            ServiceOutput::Event(actor, event) => match actor {
                ServiceControlActor::Controller(userdata) => Some(Output::Ext(ExtOut::ServicesEvent(service, userdata, event))),
                ServiceControlActor::Worker(worker, userdata) => Some(Output::Event(LogicEvent::ExtServicesEvent(worker, service, userdata, event))),
            },
            ServiceOutput::BroadcastWorkers(to) => Some(Output::Event(LogicEvent::Service(service, to))),
        }
    }
}

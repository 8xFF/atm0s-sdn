use std::sync::Arc;

use atm0s_sdn_identity::NodeId;
use rand::RngCore;

use crate::{
    base::{
        Authorization, ConnectionEvent, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, HandshakeBuilder, ServiceBuilder, ServiceControlActor, ServiceCtx,
        ServiceInput, ServiceOutput, ServiceSharedInput,
    },
    features::{FeaturesControl, FeaturesEvent},
    san_io_utils::TasksSwitcher,
    ExtIn, ExtOut, LogicControl, LogicEvent,
};

use self::{features::FeatureManager, neighbours::NeighboursManager, services::ServiceManager};

mod features;
mod neighbours;
mod services;

#[derive(Debug, Clone, convert_enum::From)]
pub enum Input<SC, TC> {
    Ext(ExtIn<SC>),
    Control(LogicControl<TC>),
    #[convert_enum(optout)]
    ShutdownRequest,
}

#[derive(Debug, Clone, convert_enum::From)]
pub enum Output<SE, TW> {
    Ext(ExtOut<SE>),
    Event(LogicEvent<TW>),
    #[convert_enum(optout)]
    ShutdownSuccess,
}

const NEIGHBOURS_ID: u8 = 0;
const FEATURES_ID: u8 = 1;
const SERVICES_ID: u8 = 2;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskType {
    Neighbours = NEIGHBOURS_ID,
    Feature = FEATURES_ID,
    Service = SERVICES_ID,
}

impl TryFrom<u8> for TaskType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            NEIGHBOURS_ID => Ok(Self::Neighbours),
            FEATURES_ID => Ok(Self::Feature),
            SERVICES_ID => Ok(Self::Service),
            _ => Err(()),
        }
    }
}

pub struct ControllerPlane<SC, SE, TC, TW> {
    tick_count: u64,
    neighbours: NeighboursManager,
    feature_ctx: FeatureContext,
    features: FeatureManager,
    service_ctx: ServiceCtx,
    services: ServiceManager<SC, SE, TC, TW>,
    switcher: TasksSwitcher<u8, 3>,
}

impl<SC, SE, TC, TW> ControllerPlane<SC, SE, TC, TW> {
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
    pub fn new(
        node_id: NodeId,
        session: u64,
        services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>,
        authorization: Arc<dyn Authorization>,
        handshake_builder: Arc<dyn HandshakeBuilder>,
        random: Box<dyn RngCore>,
    ) -> Self {
        log::info!("Create ControllerPlane for node: {}, running session {}", node_id, session);
        let service_ids = services.iter().filter(|s| s.discoverable()).map(|s| s.service_id()).collect();

        Self {
            tick_count: 0,
            neighbours: NeighboursManager::new(node_id, authorization, handshake_builder, random),
            feature_ctx: FeatureContext { node_id, session },
            features: FeatureManager::new(node_id, session, service_ids),
            service_ctx: ServiceCtx { node_id, session },
            services: ServiceManager::new(services),
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.switcher.push_all();
        self.neighbours.on_tick(now_ms, self.tick_count);
        self.features.on_shared_input(&self.feature_ctx, now_ms, FeatureSharedInput::Tick(self.tick_count));
        self.services.on_shared_input(&self.service_ctx, now_ms, ServiceSharedInput::Tick(self.tick_count));
        self.tick_count += 1;
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input<SC, TC>) {
        match event {
            Input::Ext(ExtIn::ConnectTo(addr)) => {
                self.switcher.push_last(TaskType::Neighbours as u8);
                self.neighbours.on_input(now_ms, neighbours::Input::ConnectTo(addr));
            }
            Input::Ext(ExtIn::DisconnectFrom(node)) => {
                self.switcher.push_last(TaskType::Neighbours as u8);
                self.neighbours.on_input(now_ms, neighbours::Input::DisconnectFrom(node));
            }
            Input::Ext(ExtIn::FeaturesControl(control)) => {
                self.switcher.push_last(TaskType::Feature as u8);
                self.features
                    .on_input(&self.feature_ctx, now_ms, control.to_feature(), FeatureInput::Control(FeatureControlActor::Controller, control));
            }
            Input::Ext(ExtIn::ServicesControl(service, control)) => {
                self.switcher.push_last(TaskType::Service as u8);
                self.services
                    .on_input(&self.service_ctx, now_ms, service, ServiceInput::Control(ServiceControlActor::Controller, control));
            }
            Input::Control(LogicControl::NetNeighbour(remote, control)) => {
                self.switcher.push_last(TaskType::Neighbours as u8);
                self.neighbours.on_input(now_ms, neighbours::Input::Control(remote, control));
            }
            Input::Control(LogicControl::Feature(to)) => {
                self.switcher.push_last(TaskType::Feature as u8);
                self.features.on_input(&self.feature_ctx, now_ms, to.to_feature(), FeatureInput::FromWorker(to));
            }
            Input::Control(LogicControl::Service(service, to)) => {
                self.switcher.push_last(TaskType::Service as u8);
                self.services.on_input(&self.service_ctx, now_ms, service, ServiceInput::FromWorker(to));
            }
            Input::Control(LogicControl::NetRemote(feature, conn, meta, msg)) => {
                if let Some(ctx) = self.neighbours.conn(conn) {
                    self.switcher.push_last(TaskType::Feature as u8);
                    self.features.on_input(&self.feature_ctx, now_ms, feature, FeatureInput::Net(ctx, meta, msg));
                }
            }
            Input::Control(LogicControl::NetLocal(feature, meta, msg)) => {
                self.switcher.push_last(TaskType::Feature as u8);
                self.features.on_input(&self.feature_ctx, now_ms, feature, FeatureInput::Local(meta, msg));
            }
            Input::Control(LogicControl::ServiceEvent(service, event)) => {
                self.switcher.push_last(TaskType::Service as u8);
                self.services.on_input(&self.service_ctx, now_ms, service, ServiceInput::FeatureEvent(event));
            }
            Input::Control(LogicControl::FeaturesControl(actor, control)) => {
                self.switcher.push_last(TaskType::Feature as u8);
                self.features.on_input(&self.feature_ctx, now_ms, control.to_feature(), FeatureInput::Control(actor, control));
            }
            Input::ShutdownRequest => {
                self.switcher.push_last(TaskType::Neighbours as u8);
                self.neighbours.on_input(now_ms, neighbours::Input::ShutdownRequest);
            }
        }
    }

    pub fn pop_output(&mut self, now_ms: u64) -> Option<Output<SE, TW>> {
        while let Some(current) = self.switcher.current() {
            match current.try_into().expect("Should convert to TaskType") {
                TaskType::Neighbours => {
                    let out = self.pop_neighbours(now_ms);
                    if let Some(out) = self.switcher.process(out) {
                        return Some(out);
                    }
                }
                TaskType::Feature => {
                    let out = self.pop_features(now_ms);
                    if let Some(out) = self.switcher.process(out) {
                        return Some(out);
                    }
                }
                TaskType::Service => {
                    let out = self.pop_services(now_ms);
                    if let Some(out) = self.switcher.process(out) {
                        return Some(out);
                    }
                }
            }
        }

        None
    }

    fn pop_neighbours(&mut self, now_ms: u64) -> Option<Output<SE, TW>> {
        let out = self.neighbours.pop_output(now_ms)?;
        match out {
            neighbours::Output::Control(remote, control) => Some(Output::Event(LogicEvent::NetNeighbour(remote, control))),
            neighbours::Output::Event(event) => {
                self.switcher.push_last(TaskType::Feature as u8);
                self.features.on_shared_input(&self.feature_ctx, now_ms, FeatureSharedInput::Connection(event.clone()));
                self.switcher.push_last(TaskType::Service as u8);
                self.services.on_shared_input(&self.service_ctx, now_ms, ServiceSharedInput::Connection(event.clone()));
                match event {
                    ConnectionEvent::Connected(ctx, secure) => Some(Output::Event(LogicEvent::Pin(ctx.conn, ctx.node, ctx.remote, secure))),
                    ConnectionEvent::Stats(_ctx, _stats) => None,
                    ConnectionEvent::Disconnected(ctx) => Some(Output::Event(LogicEvent::UnPin(ctx.conn))),
                }
            }
            neighbours::Output::ShutdownResponse => Some(Output::ShutdownSuccess),
        }
    }

    fn pop_features(&mut self, now_ms: u64) -> Option<Output<SE, TW>> {
        let (feature, out) = self.features.pop_output(&self.feature_ctx)?;
        match out {
            FeatureOutput::ToWorker(is_broadcast, to) => Some(Output::Event(LogicEvent::Feature(is_broadcast, to))),
            FeatureOutput::Event(actor, event) => {
                //TODO may be we need stack style for optimize performance
                log::debug!("[Controller] send FeatureEvent to actor {:?}, event {:?}", actor, event);
                match actor {
                    FeatureControlActor::Controller => Some(Output::Ext(ExtOut::FeaturesEvent(event))),
                    FeatureControlActor::Service(service) => {
                        self.switcher.push_last(TaskType::Service as u8);
                        self.services.on_input(&self.service_ctx, now_ms, service, ServiceInput::FeatureEvent(event));
                        self.pop_services(now_ms)
                    }
                }
            }
            FeatureOutput::SendDirect(conn, meta, buf) => {
                log::debug!("[ControllerPlane] SendDirect to conn: {:?}, len: {}", conn, buf.len());
                Some(Output::Event(LogicEvent::NetDirect(feature, conn, meta, buf)))
            }
            FeatureOutput::SendRoute(rule, ttl, buf) => {
                log::debug!("[ControllerPlane] SendRoute to rule: {:?}, len: {}", rule, buf.len());
                Some(Output::Event(LogicEvent::NetRoute(feature, rule, ttl, buf)))
            }
            FeatureOutput::NeighboursConnectTo(addr) => {
                self.switcher.push_last(TaskType::Neighbours as u8);
                self.neighbours.on_input(now_ms, neighbours::Input::ConnectTo(addr));
                self.pop_neighbours(now_ms)
            }
            FeatureOutput::NeighboursDisconnectFrom(node) => {
                self.switcher.push_last(TaskType::Neighbours as u8);
                self.neighbours.on_input(now_ms, neighbours::Input::DisconnectFrom(node));
                self.pop_neighbours(now_ms)
            }
        }
    }

    fn pop_services(&mut self, now_ms: u64) -> Option<Output<SE, TW>> {
        let (service, out) = self.services.pop_output(&self.service_ctx)?;
        match out {
            ServiceOutput::FeatureControl(control) => {
                self.switcher.push_last(TaskType::Feature as u8);
                self.features
                    .on_input(&self.feature_ctx, now_ms, control.to_feature(), FeatureInput::Control(FeatureControlActor::Service(service), control));
                self.pop_features(now_ms)
            }
            ServiceOutput::Event(actor, event) => match actor {
                ServiceControlActor::Controller => Some(Output::Ext(ExtOut::ServicesEvent(event))),
            },
            ServiceOutput::BroadcastWorkers(to) => Some(Output::Event(LogicEvent::Service(service, to))),
        }
    }
}

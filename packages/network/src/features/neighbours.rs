use std::{collections::VecDeque, fmt::Debug, hash::Hash};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use derivative::Derivative;
use sans_io_runtime::{collections::DynamicDeque, TaskSwitcherChild};

use crate::base::{ConnectionEvent, Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerInput, FeatureWorkerOutput};

pub const FEATURE_ID: u8 = 0;
pub const FEATURE_NAME: &str = "neighbours_api";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Sub,
    UnSub,
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Connected(NodeId, ConnId),
    Disconnected(NodeId, ConnId),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

pub type Output<UserData> = FeatureOutput<UserData, Event, ToWorker>;
pub type WorkerOutput<UserData> = FeatureWorkerOutput<UserData, Control, Event, ToController>;

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct NeighboursFeature<UserData> {
    subs: Vec<FeatureControlActor<UserData>>,
    output: VecDeque<Output<UserData>>,
}

impl<UserData: Debug + Copy + Hash + Eq> Feature<UserData, Control, Event, ToController, ToWorker> for NeighboursFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Connection(ConnectionEvent::Connected(ctx, _)) => {
                log::debug!("[Neighbours] Connected to {}, fire event to {:?}", ctx.remote, self.subs);
                for sub in self.subs.iter() {
                    self.output.push_back(FeatureOutput::Event(*sub, Event::Connected(ctx.node, ctx.conn)));
                }
            }
            FeatureSharedInput::Connection(ConnectionEvent::Disconnected(ctx)) => {
                log::debug!("[Neighbours] Disconnected to {}, fire event to {:?}", ctx.remote, self.subs);
                for sub in self.subs.iter() {
                    self.output.push_back(FeatureOutput::Event(*sub, Event::Disconnected(ctx.node, ctx.conn)));
                }
            }
            _ => {}
        }
    }

    fn on_input(&mut self, _ctx: &FeatureContext, _now_ms: u64, input: FeatureInput<'_, UserData, Control, ToController>) {
        if let FeatureInput::Control(actor, control) = input {
            match control {
                Control::Sub => {
                    if !self.subs.contains(&actor) {
                        log::info!("[Neighbours] Sub to neighbours from {:?}", actor);
                        self.subs.push(actor);
                    }
                }
                Control::UnSub => {
                    if let Some(pos) = self.subs.iter().position(|x| *x == actor) {
                        log::info!("[Neighbours] UnSub to neighbours from {:?}", actor);
                        self.subs.swap_remove(pos);
                    }
                }
                Control::ConnectTo(addr) => {
                    self.output.push_back(FeatureOutput::NeighboursConnectTo(addr));
                }
                Control::DisconnectFrom(node) => {
                    self.output.push_back(FeatureOutput::NeighboursDisconnectFrom(node));
                }
            }
        }
    }
}

impl<UserData> TaskSwitcherChild<Output<UserData>> for NeighboursFeature<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<Output<UserData>> {
        self.output.pop_front()
    }
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
pub struct NeighboursFeatureWorker<UserData> {
    queue: DynamicDeque<WorkerOutput<UserData>, 1>,
}

impl<UserData> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for NeighboursFeatureWorker<UserData> {
    fn on_input(&mut self, _ctx: &mut crate::base::FeatureWorkerContext, _now: u64, input: crate::base::FeatureWorkerInput<UserData, Control, ToWorker>) {
        match input {
            FeatureWorkerInput::Control(actor, control) => self.queue.push_back(FeatureWorkerOutput::ForwardControlToController(actor, control)),
            FeatureWorkerInput::Network(conn, header, buf) => self.queue.push_back(FeatureWorkerOutput::ForwardNetworkToController(conn, header, buf)),
            #[cfg(feature = "vpn")]
            FeatureWorkerInput::TunPkt(..) => {}
            FeatureWorkerInput::FromController(..) => {
                log::warn!("No handler for FromController");
            }
            FeatureWorkerInput::Local(header, buf) => self.queue.push_back(FeatureWorkerOutput::ForwardLocalToController(header, buf)),
        }
    }
}

impl<UserData> TaskSwitcherChild<WorkerOutput<UserData>> for NeighboursFeatureWorker<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<WorkerOutput<UserData>> {
        self.queue.pop_front()
    }
}

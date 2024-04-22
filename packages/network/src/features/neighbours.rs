use std::{collections::VecDeque, fmt::Debug, hash::Hash, marker::PhantomData};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use derivative::Derivative;

use crate::base::{ConnectionEvent, Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker};

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

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct NeighboursFeature<UserData> {
    subs: Vec<FeatureControlActor<UserData>>,
    output: VecDeque<FeatureOutput<UserData, Event, ToWorker>>,
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

    fn on_input<'a>(&mut self, _ctx: &FeatureContext, _now_ms: u64, input: FeatureInput<'a, UserData, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => match control {
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
            },
            _ => {}
        }
    }

    fn pop_output<'a>(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<UserData, Event, ToWorker>> {
        self.output.pop_front()
    }
}

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct NeighboursFeatureWorker<UserData> {
    _tmp: PhantomData<UserData>,
}

impl<UserData> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for NeighboursFeatureWorker<UserData> {}

use std::collections::VecDeque;

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};

use crate::base::{ConnectionEvent, Feature, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker};

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

#[derive(Default)]
pub struct NeighboursFeature {
    subs: Vec<FeatureControlActor>,
    output: VecDeque<FeatureOutput<Event, ToWorker>>,
}

impl Feature<Control, Event, ToController, ToWorker> for NeighboursFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, _now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Connection(ConnectionEvent::Connected(ctx, _)) => {
                for sub in self.subs.iter() {
                    self.output.push_back(FeatureOutput::Event(*sub, Event::Connected(ctx.node, ctx.conn)));
                }
            }
            FeatureSharedInput::Connection(ConnectionEvent::Disconnected(ctx)) => {
                for sub in self.subs.iter() {
                    self.output.push_back(FeatureOutput::Event(*sub, Event::Disconnected(ctx.node, ctx.conn)));
                }
            }
            _ => {}
        }
    }

    fn on_input<'a>(&mut self, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => match control {
                Control::Sub => {
                    if !self.subs.contains(&actor) {
                        log::info!("Sub to neighbours from {:?}", actor);
                        self.subs.push(actor);
                    }
                }
                Control::UnSub => {
                    if let Some(pos) = self.subs.iter().position(|x| *x == actor) {
                        log::info!("UnSub to neighbours from {:?}", actor);
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

    fn pop_output<'a>(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        self.output.pop_front()
    }
}

#[derive(Default)]
pub struct NeighboursFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for NeighboursFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }
}

use std::collections::VecDeque;

use atm0s_sdn_identity::{NodeAddr, NodeId};

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker};

pub const FEATURE_ID: u8 = 0;
pub const FEATURE_NAME: &str = "neighbours_api";

#[derive(Debug, Clone)]
pub enum Control {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
}

#[derive(Debug, Clone)]
pub enum Event {}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Default)]
pub struct NeighboursFeature {
    output: VecDeque<FeatureOutput<Event, ToWorker>>,
}

impl Feature<Control, Event, ToController, ToWorker> for NeighboursFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, _now: u64, _input: crate::base::FeatureSharedInput) {}

    fn on_input<'a>(&mut self, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Control(_service, control) => match control {
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

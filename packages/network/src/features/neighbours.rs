use std::collections::VecDeque;

use atm0s_sdn_identity::{NodeAddr, NodeId};
use log::warn;

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput};

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

    fn on_input<'a>(&mut self, now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Control(service, control) => match control {
                Control::ConnectTo(addr) => {
                    self.output.push_back(FeatureOutput::NeighboursConnectTo(addr));
                }
                Control::DisconnectFrom(node) => {
                    self.output.push_back(FeatureOutput::NeighboursDisconnectFrom(node));
                }
            },
            _ => {
                log::warn!("Invalid input for NeighboursFeature");
            }
        }
    }

    fn pop_output(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
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

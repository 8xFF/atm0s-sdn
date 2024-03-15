use atm0s_sdn_identity::{NodeAddr, NodeId};

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput};

pub const FEATURE_ID: u8 = 0;
pub const FEATURE_NAME: &str = "neighbours_api";

pub enum Control {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
}

pub enum Event {}

pub struct ToWorker;
pub struct ToController;

pub struct NeighboursFeature {}

impl Feature<Control, Event, ToController, ToWorker> for NeighboursFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_input<'a>(&mut self, now_ms: u64, input: FeatureInput<'a, Control, ToController>) {}

    fn pop_output(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        None
    }
}

pub struct NeighboursFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for NeighboursFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_tick(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64) {}

    fn on_input(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<Control, ToWorker>) -> Option<FeatureWorkerOutput<Control, Event, ToController>> {
        None
    }

    fn pop_output(&mut self) -> Option<FeatureWorkerOutput<Control, Event, ToController>> {
        None
    }
}

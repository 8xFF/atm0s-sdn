use atm0s_sdn_router::RouteRule;

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput};

pub const FEATURE_ID: u8 = 1;
pub const FEATURE_NAME: &str = "data_transfer";

pub enum Control {
    Send(RouteRule, Vec<u8>),
}

pub enum Event {
    Data(Vec<u8>),
}

pub struct ToWorker;
pub struct ToController;

pub struct DataFeature {}

impl Feature<Control, Event, ToController, ToWorker> for DataFeature {
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

pub struct DataFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for DataFeatureWorker {
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

use atm0s_sdn_router::RouteRule;

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker};

pub const FEATURE_ID: u8 = 1;
pub const FEATURE_NAME: &str = "data_transfer";

#[derive(Debug, Clone)]
pub enum Control {
    Send(RouteRule, Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum Event {
    Data(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Default)]
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

#[derive(Default)]
pub struct DataFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for DataFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }
}

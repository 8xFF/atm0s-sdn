use std::collections::HashMap;

use atm0s_sdn_identity::NodeId;

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker, ServiceId};

use self::msg::PubsubChannel;

mod msg;

pub const FEATURE_ID: u8 = 5;
pub const FEATURE_NAME: &str = "pubsub";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {}

#[derive(Debug, Clone)]
pub enum ToWorker {
    Pin(PubsubChannel, NodeId),
    Unpin(PubsubChannel, NodeId),
}

#[derive(Debug, Clone)]
pub struct ToController;

struct Slot {
    local: bool,
    hint: Option<NodeId>,
}

#[derive(Default)]
pub struct PubSubFeature {
    slots: HashMap<(ServiceId, u64), Slot>,
}

impl Feature<Control, Event, ToController, ToWorker> for PubSubFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, _now: u64, _input: crate::base::FeatureSharedInput) {}

    fn on_input<'a>(&mut self, _now_ms: u64, _input: FeatureInput<'a, Control, ToController>) {}

    fn pop_output<'a>(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        None
    }
}

#[derive(Default)]
pub struct PubSubFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for PubSubFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }
}

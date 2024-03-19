use std::collections::HashMap;

use atm0s_sdn_identity::NodeId;
use serde::{Deserialize, Serialize};

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker, ServiceId};

pub const FEATURE_ID: u8 = 4;
pub const FEATURE_NAME: &str = "lazy_kv";

#[derive(Debug, Clone)]
pub enum Control {
    Set(u64, Vec<u8>),
    Get(u64),
    Del(u64),
}

#[derive(Debug, Clone)]
pub enum LazyKvError {
    Timeout,
}

#[derive(Debug, Clone)]
pub enum Event {
    Result(u64, Result<(Vec<u8>, NodeId), LazyKvError>),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Debug, Serialize, Deserialize)]
enum Command {
    Check(u8, u64),
    CheckResult(u8, u64, Option<Vec<u8>>),
    Scan(u8, u64),
    Found(u8, u64, NodeId, Vec<u8>),
    Set(u8, u64, NodeId),
    Del(u8, u64, NodeId),
}

struct Slot {
    local: bool,
    hint: Option<NodeId>,
}

#[derive(Default)]
pub struct LazyKvFeature {
    slots: HashMap<(ServiceId, u64), Slot>,
}

impl Feature<Control, Event, ToController, ToWorker> for LazyKvFeature {
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
pub struct LazyKvFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for LazyKvFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }
}

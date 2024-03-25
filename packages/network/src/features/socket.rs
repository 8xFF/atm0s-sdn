use crate::base::{Feature, FeatureWorker};

pub const FEATURE_ID: u8 = 7;
pub const FEATURE_NAME: &str = "socket";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Default)]
pub struct SocketFeature {}

impl Feature<Control, Event, ToController, ToWorker> for SocketFeature {
    fn on_shared_input(&mut self, _ctx: &crate::base::FeatureContext, _now: u64, _input: crate::base::FeatureSharedInput) {
        todo!()
    }

    fn on_input<'a>(&mut self, _ctx: &crate::base::FeatureContext, now_ms: u64, input: crate::base::FeatureInput<'a, Control, ToController>) {
        todo!()
    }

    fn pop_output(&mut self, _ctx: &crate::base::FeatureContext) -> Option<crate::base::FeatureOutput<Event, ToWorker>> {
        todo!()
    }
}

pub struct SocketFeatureWorker;

impl FeatureWorker<Control, Event, ToController, ToWorker> for SocketFeatureWorker {}

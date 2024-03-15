use crate::base::{FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput};
use crate::features::*;

pub type FeaturesWorkerInput = FeatureWorkerInput<FeaturesControl, FeaturesToWorker>;
pub type FeaturesWorkerOutput = FeatureWorkerOutput<FeaturesControl, FeaturesEvent, FeaturesToController>;

use crate::san_io_utils::TasksSwitcher;

///
/// FeatureWorkerManager is a manager for all features
/// This will take-care of how to route the input to the correct feature
/// With some special event must to broadcast to all features (Tick, Transport Events), it will
/// use a switcher to correctly process one by one
///
pub struct FeatureWorkerManager {
    neighbours: neighbours::NeighboursFeatureWorker,
    data: data::DataFeatureWorker,
    last_input_feature: Option<u8>,
    switcher: TasksSwitcher<2>,
}

impl FeatureWorkerManager {
    pub fn new() -> Self {
        todo!()
    }

    pub fn on_tick(&mut self, ctx: &mut FeatureWorkerContext, now_ms: u64) {
        self.last_input_feature = None;
        self.neighbours.on_tick(ctx, now_ms);
        self.data.on_tick(ctx, now_ms);
    }

    pub fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, now_ms: u64, feature: u8, input: FeaturesWorkerInput) -> Option<FeaturesWorkerOutput> {
        match input {
            FeatureWorkerInput::Control(service, control) => match control {
                FeaturesControl::Data(control) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::Neighbours(control) => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
            },
            FeatureWorkerInput::FromController(to) => match to {
                FeaturesToWorker::Data(to) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
                FeaturesToWorker::Neighbours(to) => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
            },
            FeatureWorkerInput::Network(conn, msg) => match feature {
                data::FEATURE_ID => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Network(conn, msg)).map(|a| a.into2()),
                neighbours::FEATURE_ID => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::Network(conn, msg)).map(|a| a.into2()),
                _ => None,
            },
        }
    }

    pub fn pop_output(&mut self) -> Option<FeaturesWorkerOutput> {
        if let Some(last_feature) = self.last_input_feature {
            match last_feature {
                data::FEATURE_ID => self.data.pop_output().map(|a| a.into2()),
                neighbours::FEATURE_ID => self.neighbours.pop_output().map(|a| a.into2()),
                _ => None,
            }
        } else {
            loop {
                let s = &mut self.switcher;
                match s.current()? as u8 {
                    neighbours::FEATURE_ID => {
                        if let Some(out) = s.process(self.neighbours.pop_output()) {
                            return Some(out.into2());
                        }
                    }
                    data::FEATURE_ID => {
                        if let Some(out) = s.process(self.data.pop_output()) {
                            return Some(out.into2());
                        }
                    }
                    _ => return None,
                }
            }
        }
    }
}

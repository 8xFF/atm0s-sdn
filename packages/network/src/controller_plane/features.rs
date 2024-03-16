use atm0s_sdn_identity::NodeId;

use crate::base::{Feature, FeatureInput, FeatureOutput};
use crate::features::*;

pub type FeaturesInput<'a> = FeatureInput<'a, FeaturesControl, FeaturesToController>;
pub type FeaturesOutput = FeatureOutput<FeaturesEvent, FeaturesToWorker>;

use crate::san_io_utils::TasksSwitcher;

///
/// FeatureManager is a manager for all features
/// This will take-care of how to route the input to the correct feature
/// With some special event must to broadcast to all features (Tick, Transport Events), it will
/// use a switcher to correctly process one by one
///
pub struct FeatureManager {
    neighbours: neighbours::NeighboursFeature,
    data: data::DataFeature,
    router_sync: router_sync::RouterSyncFeature,
    switcher: TasksSwitcher<3>,
    last_input_feature: Option<u8>,
}

impl FeatureManager {
    pub fn new(node: NodeId) -> Self {
        Self {
            neighbours: neighbours::NeighboursFeature::default(),
            data: data::DataFeature::default(),
            router_sync: router_sync::RouterSyncFeature::new(node),
            last_input_feature: None,
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_input<'a>(&mut self, now_ms: u64, input: FeaturesInput<'a>) {
        match input {
            FeatureInput::Shared(event) => {
                self.neighbours.on_input(now_ms, FeatureInput::Shared(event.clone()));
                self.data.on_input(now_ms, FeatureInput::Shared(event.clone()));
                self.router_sync.on_input(now_ms, FeatureInput::Shared(event));
                self.last_input_feature = None;
            }
            FeatureInput::FromWorker(to) => match to {
                FeaturesToController::Data(to) => {
                    self.last_input_feature = Some(data::FEATURE_ID);
                    self.data.on_input(now_ms, FeatureInput::FromWorker(to))
                }
                FeaturesToController::Neighbours(to) => {
                    self.last_input_feature = Some(neighbours::FEATURE_ID);
                    self.neighbours.on_input(now_ms, FeatureInput::FromWorker(to))
                }
                FeaturesToController::RouterSync(to) => {
                    self.last_input_feature = Some(router_sync::FEATURE_ID);
                    self.router_sync.on_input(now_ms, FeatureInput::FromWorker(to))
                }
            },
            FeatureInput::Control(service, control) => match control {
                FeaturesControl::Data(control) => {
                    self.last_input_feature = Some(data::FEATURE_ID);
                    self.data.on_input(now_ms, FeatureInput::Control(service, control))
                }
                FeaturesControl::Neighbours(control) => {
                    self.last_input_feature = Some(neighbours::FEATURE_ID);
                    self.neighbours.on_input(now_ms, FeatureInput::Control(service, control))
                }
                FeaturesControl::RouterSync(control) => {
                    self.last_input_feature = Some(router_sync::FEATURE_ID);
                    self.router_sync.on_input(now_ms, FeatureInput::Control(service, control))
                }
            },
            FeatureInput::ForwardNetFromWorker(ctx, msg) => match msg.header.feature {
                data::FEATURE_ID => {
                    self.last_input_feature = Some(data::FEATURE_ID);
                    self.data.on_input(now_ms, FeatureInput::ForwardNetFromWorker(ctx, msg))
                }
                neighbours::FEATURE_ID => {
                    self.last_input_feature = Some(neighbours::FEATURE_ID);
                    self.neighbours.on_input(now_ms, FeatureInput::ForwardNetFromWorker(ctx, msg))
                }
                router_sync::FEATURE_ID => {
                    self.last_input_feature = Some(router_sync::FEATURE_ID);
                    self.router_sync.on_input(now_ms, FeatureInput::ForwardNetFromWorker(ctx, msg))
                }
                _ => {}
            },
        }
    }

    pub fn pop_output(&mut self) -> Option<FeaturesOutput> {
        if let Some(last_feature) = self.last_input_feature {
            let res = match last_feature {
                data::FEATURE_ID => self.data.pop_output().map(|a| a.into2()),
                neighbours::FEATURE_ID => self.neighbours.pop_output().map(|a| a.into2()),
                router_sync::FEATURE_ID => self.router_sync.pop_output().map(|a| a.into2()),
                _ => None,
            };
            if res.is_none() {
                self.last_input_feature = None;
            }
            res
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
                    router_sync::FEATURE_ID => {
                        if let Some(out) = s.process(self.router_sync.pop_output()) {
                            return Some(out.into2());
                        }
                    }
                    _ => return None,
                }
            }
        }
    }
}

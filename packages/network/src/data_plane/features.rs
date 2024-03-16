use atm0s_sdn_identity::ConnId;

use crate::base::{FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer};
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
    router_sync: router_sync::RouterSyncFeatureWorker,
    last_input_feature: Option<u8>,
    switcher: TasksSwitcher<256>,
}

impl FeatureWorkerManager {
    pub fn new() -> Self {
        Self {
            neighbours: neighbours::NeighboursFeatureWorker::default(),
            data: data::DataFeatureWorker::default(),
            router_sync: router_sync::RouterSyncFeatureWorker::default(),
            last_input_feature: None,
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_tick(&mut self, ctx: &mut FeatureWorkerContext, now_ms: u64) {
        self.last_input_feature = None;
        self.neighbours.on_tick(ctx, now_ms);
        self.data.on_tick(ctx, now_ms);
    }

    pub fn on_network_raw<'a>(&mut self, ctx: &mut FeatureWorkerContext, feature: u8, now_ms: u64, conn: ConnId, header_len: usize, buf: GenericBuffer<'a>) -> Option<(u8, FeaturesWorkerOutput)> {
        match feature {
            neighbours::FEATURE_ID => self.neighbours.on_network_raw(ctx, now_ms, conn, header_len, buf).map(|a| (neighbours::FEATURE_ID, a.into2())),
            data::FEATURE_ID => self.data.on_network_raw(ctx, now_ms, conn, header_len, buf).map(|a| (data::FEATURE_ID, a.into2())),
            router_sync::FEATURE_ID => self.router_sync.on_network_raw(ctx, now_ms, conn, header_len, buf).map(|a| (router_sync::FEATURE_ID, a.into2())),
            _ => None,
        }
    }

    pub fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, feature: u8, now_ms: u64, input: FeaturesWorkerInput) -> Option<(u8, FeaturesWorkerOutput)> {
        match input {
            FeatureWorkerInput::Control(service, control) => match control {
                FeaturesControl::Neighbours(control) => self
                    .neighbours
                    .on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control))
                    .map(|a| (neighbours::FEATURE_ID, a.into2())),
                FeaturesControl::Data(control) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| (data::FEATURE_ID, a.into2())),
                FeaturesControl::RouterSync(control) => self
                    .router_sync
                    .on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control))
                    .map(|a| (router_sync::FEATURE_ID, a.into2())),
            },
            FeatureWorkerInput::FromController(to) => match to {
                FeaturesToWorker::Neighbours(to) => self
                    .neighbours
                    .on_input(ctx, now_ms, FeatureWorkerInput::FromController(to))
                    .map(|a| (neighbours::FEATURE_ID, a.into2())),
                FeaturesToWorker::Data(to) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| (data::FEATURE_ID, a.into2())),
                FeaturesToWorker::RouterSync(to) => self
                    .router_sync
                    .on_input(ctx, now_ms, FeatureWorkerInput::FromController(to))
                    .map(|a| (router_sync::FEATURE_ID, a.into2())),
            },
            FeatureWorkerInput::Network(_conn, _buf) => {
                panic!("should call above on_network_raw")
            }
            FeatureWorkerInput::Local(buf) => match feature {
                neighbours::FEATURE_ID => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| (neighbours::FEATURE_ID, a.into2())),
                data::FEATURE_ID => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| (data::FEATURE_ID, a.into2())),
                router_sync::FEATURE_ID => self.router_sync.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| (router_sync::FEATURE_ID, a.into2())),
                _ => None,
            },
        }
    }

    pub fn pop_output(&mut self) -> Option<(u8, FeaturesWorkerOutput)> {
        if let Some(last_feature) = self.last_input_feature {
            match last_feature {
                neighbours::FEATURE_ID => self.neighbours.pop_output().map(|a| (neighbours::FEATURE_ID, a.into2())),
                data::FEATURE_ID => self.data.pop_output().map(|a| (data::FEATURE_ID, a.into2())),
                router_sync::FEATURE_ID => self.router_sync.pop_output().map(|a| (router_sync::FEATURE_ID, a.into2())),
                _ => None,
            }
        } else {
            loop {
                let s = &mut self.switcher;
                match s.current()? as u8 {
                    neighbours::FEATURE_ID => {
                        if let Some(out) = s.process(self.neighbours.pop_output()) {
                            return Some((neighbours::FEATURE_ID, out.into2()));
                        }
                    }
                    data::FEATURE_ID => {
                        if let Some(out) = s.process(self.data.pop_output()) {
                            return Some((data::FEATURE_ID, out.into2()));
                        }
                    }
                    router_sync::FEATURE_ID => {
                        if let Some(out) = s.process(self.router_sync.pop_output()) {
                            return Some((router_sync::FEATURE_ID, out.into2()));
                        }
                    }
                    _ => return None,
                }
            }
        }
    }
}

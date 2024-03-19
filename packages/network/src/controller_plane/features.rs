use atm0s_sdn_identity::NodeId;

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureSharedInput};
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
    vpn: vpn::VpnFeature,
    dht_kv: dht_kv::DhtKvFeature,
    switcher: TasksSwitcher<5>,
    last_input_feature: Option<Features>,
}

impl FeatureManager {
    pub fn new(node: NodeId, session: u64) -> Self {
        Self {
            neighbours: neighbours::NeighboursFeature::default(),
            data: data::DataFeature::new(node),
            router_sync: router_sync::RouterSyncFeature::new(node),
            vpn: vpn::VpnFeature::default(),
            dht_kv: dht_kv::DhtKvFeature::new(node, session),
            last_input_feature: None,
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_shared_input<'a>(&mut self, now_ms: u64, input: FeatureSharedInput) {
        self.data.on_shared_input(now_ms, input.clone());
        self.neighbours.on_shared_input(now_ms, input.clone());
        self.router_sync.on_shared_input(now_ms, input.clone());
        self.dht_kv.on_shared_input(now_ms, input.clone());
        self.vpn.on_shared_input(now_ms, input);
        self.last_input_feature = None;
    }

    pub fn on_input<'a>(&mut self, now_ms: u64, feature: Features, input: FeaturesInput<'a>) {
        match input {
            FeatureInput::FromWorker(to) => match to {
                FeaturesToController::Data(to) => {
                    self.last_input_feature = Some(Features::Data);
                    self.data.on_input(now_ms, FeatureInput::FromWorker(to))
                }
                FeaturesToController::Neighbours(to) => {
                    self.last_input_feature = Some(Features::Neighbours);
                    self.neighbours.on_input(now_ms, FeatureInput::FromWorker(to))
                }
                FeaturesToController::RouterSync(to) => {
                    self.last_input_feature = Some(Features::RouterSync);
                    self.router_sync.on_input(now_ms, FeatureInput::FromWorker(to))
                }
                FeaturesToController::Vpn(to) => {
                    self.last_input_feature = Some(Features::Vpn);
                    self.vpn.on_input(now_ms, FeatureInput::FromWorker(to))
                }
                FeaturesToController::DhtKv(to) => {
                    self.last_input_feature = Some(Features::DhtKv);
                    self.dht_kv.on_input(now_ms, FeatureInput::FromWorker(to))
                }
            },
            FeatureInput::Control(service, control) => match control {
                FeaturesControl::Data(control) => {
                    self.last_input_feature = Some(Features::Data);
                    self.data.on_input(now_ms, FeatureInput::Control(service, control))
                }
                FeaturesControl::Neighbours(control) => {
                    self.last_input_feature = Some(Features::Neighbours);
                    self.neighbours.on_input(now_ms, FeatureInput::Control(service, control))
                }
                FeaturesControl::RouterSync(control) => {
                    self.last_input_feature = Some(Features::RouterSync);
                    self.router_sync.on_input(now_ms, FeatureInput::Control(service, control))
                }
                FeaturesControl::Vpn(control) => {
                    self.last_input_feature = Some(Features::Vpn);
                    self.vpn.on_input(now_ms, FeatureInput::Control(service, control))
                }
                FeaturesControl::DhtKv(control) => {
                    self.last_input_feature = Some(Features::DhtKv);
                    self.dht_kv.on_input(now_ms, FeatureInput::Control(service, control))
                }
            },
            FeatureInput::Net(ctx, buf) => match feature {
                Features::Data => {
                    self.last_input_feature = Some(Features::Data);
                    self.data.on_input(now_ms, FeatureInput::Net(ctx, buf))
                }
                Features::Neighbours => {
                    self.last_input_feature = Some(Features::Neighbours);
                    self.neighbours.on_input(now_ms, FeatureInput::Net(ctx, buf))
                }
                Features::RouterSync => {
                    self.last_input_feature = Some(Features::RouterSync);
                    self.router_sync.on_input(now_ms, FeatureInput::Net(ctx, buf))
                }
                Features::Vpn => {
                    self.last_input_feature = Some(Features::Vpn);
                    self.vpn.on_input(now_ms, FeatureInput::Net(ctx, buf))
                }
                Features::DhtKv => {
                    self.last_input_feature = Some(Features::DhtKv);
                    self.dht_kv.on_input(now_ms, FeatureInput::Net(ctx, buf))
                }
            },
            FeatureInput::Local(buf) => match feature {
                Features::Data => {
                    self.last_input_feature = Some(Features::Data);
                    self.data.on_input(now_ms, FeatureInput::Local(buf))
                }
                Features::Neighbours => {
                    self.last_input_feature = Some(Features::Neighbours);
                    self.neighbours.on_input(now_ms, FeatureInput::Local(buf))
                }
                Features::RouterSync => {
                    self.last_input_feature = Some(Features::RouterSync);
                    self.router_sync.on_input(now_ms, FeatureInput::Local(buf))
                }
                Features::Vpn => {
                    self.last_input_feature = Some(Features::Vpn);
                    self.vpn.on_input(now_ms, FeatureInput::Local(buf))
                }
                Features::DhtKv => {
                    self.last_input_feature = Some(Features::DhtKv);
                    self.dht_kv.on_input(now_ms, FeatureInput::Local(buf))
                }
            },
        }
    }

    pub fn pop_output<'a>(&mut self) -> Option<(Features, FeaturesOutput)> {
        if let Some(last_feature) = self.last_input_feature {
            let res = match last_feature {
                Features::Data => self.data.pop_output().map(|a| (Features::Data, a.into2())),
                Features::Neighbours => self.neighbours.pop_output().map(|a| (Features::Neighbours, a.into2())),
                Features::RouterSync => self.router_sync.pop_output().map(|a| (Features::RouterSync, a.into2())),
                Features::Vpn => self.vpn.pop_output().map(|a| (Features::Vpn, a.into2())),
                Features::DhtKv => self.dht_kv.pop_output().map(|a| (Features::DhtKv, a.into2())),
            };
            if res.is_none() {
                self.last_input_feature = None;
            }
            res
        } else {
            let s = &mut self.switcher;
            loop {
                match (s.current()? as u8).try_into().ok()? {
                    Features::Neighbours => {
                        if let Some(out) = s.process(self.neighbours.pop_output()) {
                            return Some((Features::Neighbours, out.into2()));
                        }
                    }
                    Features::Data => {
                        if let Some(out) = s.process(self.data.pop_output()) {
                            return Some((Features::Data, out.into2()));
                        }
                    }
                    Features::RouterSync => {
                        if let Some(out) = s.process(self.router_sync.pop_output()) {
                            return Some((Features::RouterSync, out.into2()));
                        }
                    }
                    Features::Vpn => {
                        if let Some(out) = s.process(self.vpn.pop_output()) {
                            return Some((Features::Vpn, out.into2()));
                        }
                    }
                    Features::DhtKv => {
                        if let Some(out) = s.process(self.dht_kv.pop_output()) {
                            return Some((Features::DhtKv, out.into2()));
                        }
                    }
                }
            }
        }
    }
}

use atm0s_sdn_identity::NodeId;

use crate::base::{Feature, FeatureContext, FeatureInput, FeatureOutput, FeatureSharedInput};
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
    pubsub: pubsub::PubSubFeature,
    switcher: TasksSwitcher<u8, 6>,
}

impl FeatureManager {
    pub fn new(node: NodeId, session: u64, services: Vec<u8>) -> Self {
        Self {
            neighbours: neighbours::NeighboursFeature::default(),
            data: data::DataFeature::default(),
            router_sync: router_sync::RouterSyncFeature::new(node, services),
            vpn: vpn::VpnFeature::default(),
            dht_kv: dht_kv::DhtKvFeature::new(node, session),
            pubsub: pubsub::PubSubFeature::new(),
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_shared_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, input: FeatureSharedInput) {
        self.switcher.push_all();
        self.data.on_shared_input(ctx, now_ms, input.clone());
        self.neighbours.on_shared_input(ctx, now_ms, input.clone());
        self.router_sync.on_shared_input(ctx, now_ms, input.clone());
        self.dht_kv.on_shared_input(ctx, now_ms, input.clone());
        self.vpn.on_shared_input(ctx, now_ms, input.clone());
        self.pubsub.on_shared_input(ctx, now_ms, input);
    }

    pub fn on_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, feature: Features, input: FeaturesInput<'a>) {
        self.switcher.push_last(feature as u8);
        match input {
            FeatureInput::FromWorker(to) => match to {
                FeaturesToController::Data(to) => self.data.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Neighbours(to) => self.neighbours.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::RouterSync(to) => self.router_sync.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Vpn(to) => self.vpn.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::DhtKv(to) => self.dht_kv.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::PubSub(to) => self.pubsub.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
            },
            FeatureInput::Control(service, control) => match control {
                FeaturesControl::Data(control) => self.data.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Neighbours(control) => self.neighbours.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::RouterSync(control) => self.router_sync.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Vpn(control) => self.vpn.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::DhtKv(control) => self.dht_kv.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::PubSub(control) => self.pubsub.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
            },
            FeatureInput::Net(con_ctx, buf) => match feature {
                Features::Data => self.data.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, buf)),
                Features::Neighbours => self.neighbours.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, buf)),
                Features::RouterSync => self.router_sync.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, buf)),
                Features::Vpn => self.vpn.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, buf)),
                Features::DhtKv => self.dht_kv.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, buf)),
                Features::PubSub => self.pubsub.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, buf)),
            },
            FeatureInput::Local(buf) => match feature {
                Features::Data => self.data.on_input(ctx, now_ms, FeatureInput::Local(buf)),
                Features::Neighbours => self.neighbours.on_input(ctx, now_ms, FeatureInput::Local(buf)),
                Features::RouterSync => self.router_sync.on_input(ctx, now_ms, FeatureInput::Local(buf)),
                Features::Vpn => self.vpn.on_input(ctx, now_ms, FeatureInput::Local(buf)),
                Features::DhtKv => self.dht_kv.on_input(ctx, now_ms, FeatureInput::Local(buf)),
                Features::PubSub => self.pubsub.on_input(ctx, now_ms, FeatureInput::Local(buf)),
            },
        }
    }

    pub fn pop_output<'a>(&mut self, ctx: &FeatureContext) -> Option<(Features, FeaturesOutput)> {
        let s = &mut self.switcher;
        loop {
            match (s.current()? as u8).try_into().ok()? {
                Features::Neighbours => {
                    if let Some(out) = s.process(self.neighbours.pop_output(ctx)) {
                        return Some((Features::Neighbours, out.into2()));
                    }
                }
                Features::Data => {
                    if let Some(out) = s.process(self.data.pop_output(ctx)) {
                        return Some((Features::Data, out.into2()));
                    }
                }
                Features::RouterSync => {
                    if let Some(out) = s.process(self.router_sync.pop_output(ctx)) {
                        return Some((Features::RouterSync, out.into2()));
                    }
                }
                Features::Vpn => {
                    if let Some(out) = s.process(self.vpn.pop_output(ctx)) {
                        return Some((Features::Vpn, out.into2()));
                    }
                }
                Features::DhtKv => {
                    if let Some(out) = s.process(self.dht_kv.pop_output(ctx)) {
                        return Some((Features::DhtKv, out.into2()));
                    }
                }
                Features::PubSub => {
                    if let Some(out) = s.process(self.pubsub.pop_output(ctx)) {
                        return Some((Features::PubSub, out.into2()));
                    }
                }
            }
        }
    }
}

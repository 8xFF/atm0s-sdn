use std::fmt::Debug;
use std::hash::Hash;

use atm0s_sdn_identity::NodeId;
use sans_io_runtime::TaskSwitcher;

use crate::base::{Feature, FeatureContext, FeatureInput, FeatureOutput, FeatureSharedInput};
use crate::features::*;

pub type FeaturesInput<'a, UserData> = FeatureInput<'a, UserData, FeaturesControl, FeaturesToController>;
pub type FeaturesOutput<UserData> = FeatureOutput<UserData, FeaturesEvent, FeaturesToWorker<UserData>>;

///
/// FeatureManager is a manager for all features
/// This will take-care of how to route the input to the correct feature
/// With some special event must to broadcast to all features (Tick, Transport Events), it will
/// use a switcher to correctly process one by one
///
pub struct FeatureManager<UserData> {
    neighbours: neighbours::NeighboursFeature<UserData>,
    data: data::DataFeature<UserData>,
    router_sync: router_sync::RouterSyncFeature<UserData>,
    vpn: vpn::VpnFeature<UserData>,
    dht_kv: dht_kv::DhtKvFeature<UserData>,
    pubsub: pubsub::PubSubFeature<UserData>,
    alias: alias::AliasFeature<UserData>,
    socket: socket::SocketFeature<UserData>,
    switcher: TaskSwitcher,
}

impl<UserData: 'static + Hash + Eq + Copy + Debug> FeatureManager<UserData> {
    pub fn new(node: NodeId, session: u64, services: Vec<u8>) -> Self {
        Self {
            neighbours: neighbours::NeighboursFeature::default(),
            data: data::DataFeature::default(),
            router_sync: router_sync::RouterSyncFeature::new(node, services),
            vpn: vpn::VpnFeature::default(),
            dht_kv: dht_kv::DhtKvFeature::new(node, session),
            pubsub: pubsub::PubSubFeature::new(),
            alias: alias::AliasFeature::default(),
            socket: socket::SocketFeature::default(),
            switcher: TaskSwitcher::new(8),
        }
    }

    pub fn on_shared_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, input: FeatureSharedInput) {
        self.switcher.queue_flag_all();
        self.data.on_shared_input(ctx, now_ms, input.clone());
        self.neighbours.on_shared_input(ctx, now_ms, input.clone());
        self.router_sync.on_shared_input(ctx, now_ms, input.clone());
        self.dht_kv.on_shared_input(ctx, now_ms, input.clone());
        self.vpn.on_shared_input(ctx, now_ms, input.clone());
        self.pubsub.on_shared_input(ctx, now_ms, input.clone());
        self.alias.on_shared_input(ctx, now_ms, input.clone());
        self.socket.on_shared_input(ctx, now_ms, input);
    }

    pub fn on_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, feature: Features, input: FeaturesInput<'a, UserData>) {
        self.switcher.queue_flag_task(feature as usize);
        match input {
            FeatureInput::FromWorker(to) => match to {
                FeaturesToController::Data(to) => self.data.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Neighbours(to) => self.neighbours.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::RouterSync(to) => self.router_sync.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Vpn(to) => self.vpn.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::DhtKv(to) => self.dht_kv.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::PubSub(to) => self.pubsub.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Alias(to) => self.alias.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Socket(to) => self.socket.on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
            },
            FeatureInput::Control(service, control) => match control {
                FeaturesControl::Data(control) => self.data.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Neighbours(control) => self.neighbours.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::RouterSync(control) => self.router_sync.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Vpn(control) => self.vpn.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::DhtKv(control) => self.dht_kv.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::PubSub(control) => self.pubsub.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Alias(control) => self.alias.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Socket(control) => self.socket.on_input(ctx, now_ms, FeatureInput::Control(service, control)),
            },
            FeatureInput::Net(con_ctx, header, buf) => match feature {
                Features::Data => self.data.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Neighbours => self.neighbours.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::RouterSync => self.router_sync.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Vpn => self.vpn.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::DhtKv => self.dht_kv.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::PubSub => self.pubsub.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Alias => self.alias.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Socket => self.socket.on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
            },
            FeatureInput::Local(header, buf) => match feature {
                Features::Data => self.data.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Neighbours => self.neighbours.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::RouterSync => self.router_sync.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Vpn => self.vpn.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::DhtKv => self.dht_kv.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::PubSub => self.pubsub.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Alias => self.alias.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Socket => self.socket.on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
            },
        }
    }

    pub fn pop_output<'a>(&mut self, ctx: &FeatureContext) -> Option<(Features, FeaturesOutput<UserData>)> {
        let s = &mut self.switcher;
        loop {
            match (s.queue_current()? as u8).try_into().ok()? {
                Features::Neighbours => {
                    if let Some(out) = s.queue_process(self.neighbours.pop_output(ctx)) {
                        return Some((Features::Neighbours, out.into2()));
                    }
                }
                Features::Data => {
                    if let Some(out) = s.queue_process(self.data.pop_output(ctx)) {
                        return Some((Features::Data, out.into2()));
                    }
                }
                Features::RouterSync => {
                    if let Some(out) = s.queue_process(self.router_sync.pop_output(ctx)) {
                        return Some((Features::RouterSync, out.into2()));
                    }
                }
                Features::Vpn => {
                    if let Some(out) = s.queue_process(self.vpn.pop_output(ctx)) {
                        return Some((Features::Vpn, out.into2()));
                    }
                }
                Features::DhtKv => {
                    if let Some(out) = s.queue_process(self.dht_kv.pop_output(ctx)) {
                        return Some((Features::DhtKv, out.into2()));
                    }
                }
                Features::PubSub => {
                    if let Some(out) = s.queue_process(self.pubsub.pop_output(ctx)) {
                        return Some((Features::PubSub, out.into2()));
                    }
                }
                Features::Alias => {
                    if let Some(out) = s.queue_process(self.alias.pop_output(ctx)) {
                        return Some((Features::Alias, out.into2()));
                    }
                }
                Features::Socket => {
                    if let Some(out) = s.queue_process(self.socket.pop_output(ctx)) {
                        return Some((Features::Socket, out.into2()));
                    }
                }
            }
        }
    }
}

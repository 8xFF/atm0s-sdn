use std::fmt::Debug;
use std::hash::Hash;

use atm0s_sdn_identity::NodeId;
use sans_io_runtime::{TaskSwitcher, TaskSwitcherBranch, TaskSwitcherChild};

use crate::base::{Feature, FeatureContext, FeatureInput, FeatureOutput, FeatureSharedInput};
use crate::features::*;

pub type FeaturesInput<'a, UserData> = FeatureInput<'a, UserData, FeaturesControl, FeaturesToController>;
pub type FeaturesOutput<UserData> = FeatureOutput<UserData, FeaturesEvent, FeaturesToWorker<UserData>>;

pub type Output<UserData> = (Features, FeaturesOutput<UserData>);

///
/// FeatureManager is a manager for all features
/// This will take-care of how to route the input to the correct feature
/// With some special event must to broadcast to all features (Tick, Transport Events), it will
/// use a switcher to correctly process one by one
///
pub struct FeatureManager<UserData> {
    neighbours: TaskSwitcherBranch<neighbours::NeighboursFeature<UserData>, neighbours::Output<UserData>>,
    data: TaskSwitcherBranch<data::DataFeature<UserData>, data::Output<UserData>>,
    router_sync: TaskSwitcherBranch<router_sync::RouterSyncFeature<UserData>, router_sync::Output<UserData>>,
    vpn: TaskSwitcherBranch<vpn::VpnFeature<UserData>, vpn::Output<UserData>>,
    dht_kv: TaskSwitcherBranch<dht_kv::DhtKvFeature<UserData>, dht_kv::Output<UserData>>,
    pubsub: TaskSwitcherBranch<pubsub::PubSubFeature<UserData>, pubsub::Output<UserData>>,
    alias: TaskSwitcherBranch<alias::AliasFeature<UserData>, alias::Output<UserData>>,
    socket: TaskSwitcherBranch<socket::SocketFeature<UserData>, socket::Output<UserData>>,
    switcher: TaskSwitcher,
}

impl<UserData: 'static + Hash + Eq + Copy + Debug> FeatureManager<UserData> {
    pub fn new(node: NodeId, session: u64, services: Vec<u8>) -> Self {
        Self {
            neighbours: TaskSwitcherBranch::default(Features::Neighbours as usize),
            data: TaskSwitcherBranch::default(Features::Data as usize),
            router_sync: TaskSwitcherBranch::new(router_sync::RouterSyncFeature::new(node, services), Features::RouterSync as usize),
            vpn: TaskSwitcherBranch::default(Features::Vpn as usize),
            dht_kv: TaskSwitcherBranch::new(dht_kv::DhtKvFeature::new(node, session), Features::DhtKv as usize),
            pubsub: TaskSwitcherBranch::new(pubsub::PubSubFeature::new(), Features::PubSub as usize),
            alias: TaskSwitcherBranch::default(Features::Alias as usize),
            socket: TaskSwitcherBranch::default(Features::Socket as usize),
            switcher: TaskSwitcher::new(8),
        }
    }

    pub fn on_shared_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, input: FeatureSharedInput) {
        self.data.input(&mut self.switcher).on_shared_input(ctx, now_ms, input.clone());
        self.neighbours.input(&mut self.switcher).on_shared_input(ctx, now_ms, input.clone());
        self.router_sync.input(&mut self.switcher).on_shared_input(ctx, now_ms, input.clone());
        self.dht_kv.input(&mut self.switcher).on_shared_input(ctx, now_ms, input.clone());
        self.vpn.input(&mut self.switcher).on_shared_input(ctx, now_ms, input.clone());
        self.pubsub.input(&mut self.switcher).on_shared_input(ctx, now_ms, input.clone());
        self.alias.input(&mut self.switcher).on_shared_input(ctx, now_ms, input.clone());
        self.socket.input(&mut self.switcher).on_shared_input(ctx, now_ms, input);
    }

    pub fn on_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, feature: Features, input: FeaturesInput<'a, UserData>) {
        match input {
            FeatureInput::FromWorker(to) => match to {
                FeaturesToController::Data(to) => self.data.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Neighbours(to) => self.neighbours.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::RouterSync(to) => self.router_sync.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Vpn(to) => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::DhtKv(to) => self.dht_kv.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::PubSub(to) => self.pubsub.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Alias(to) => self.alias.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
                FeaturesToController::Socket(to) => self.socket.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::FromWorker(to)),
            },
            FeatureInput::Control(service, control) => match control {
                FeaturesControl::Data(control) => self.data.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Neighbours(control) => self.neighbours.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::RouterSync(control) => self.router_sync.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Vpn(control) => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::DhtKv(control) => self.dht_kv.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::PubSub(control) => self.pubsub.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Alias(control) => self.alias.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
                FeaturesControl::Socket(control) => self.socket.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Control(service, control)),
            },
            FeatureInput::Net(con_ctx, header, buf) => match feature {
                Features::Data => self.data.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Neighbours => self.neighbours.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::RouterSync => self.router_sync.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Vpn => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::DhtKv => self.dht_kv.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::PubSub => self.pubsub.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Alias => self.alias.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
                Features::Socket => self.socket.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Net(con_ctx, header, buf)),
            },
            FeatureInput::Local(header, buf) => match feature {
                Features::Data => self.data.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Neighbours => self.neighbours.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::RouterSync => self.router_sync.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Vpn => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::DhtKv => self.dht_kv.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::PubSub => self.pubsub.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Alias => self.alias.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
                Features::Socket => self.socket.input(&mut self.switcher).on_input(ctx, now_ms, FeatureInput::Local(header, buf)),
            },
        }
    }
}

impl<UserData: Hash + Eq + Copy + Debug> TaskSwitcherChild<Output<UserData>> for FeatureManager<UserData> {
    type Time = u64;
    fn pop_output<'a>(&mut self, now: u64) -> Option<Output<UserData>> {
        loop {
            match (self.switcher.current()? as u8).try_into().ok()? {
                Features::Neighbours => {
                    if let Some(out) = self.neighbours.pop_output(now, &mut self.switcher) {
                        return Some((Features::Neighbours, out.into2()));
                    }
                }
                Features::Data => {
                    if let Some(out) = self.data.pop_output(now, &mut self.switcher) {
                        return Some((Features::Data, out.into2()));
                    }
                }
                Features::RouterSync => {
                    if let Some(out) = self.router_sync.pop_output(now, &mut self.switcher) {
                        return Some((Features::RouterSync, out.into2()));
                    }
                }
                Features::Vpn => {
                    if let Some(out) = self.vpn.pop_output(now, &mut self.switcher) {
                        return Some((Features::Vpn, out.into2()));
                    }
                }
                Features::DhtKv => {
                    if let Some(out) = self.dht_kv.pop_output(now, &mut self.switcher) {
                        return Some((Features::DhtKv, out.into2()));
                    }
                }
                Features::PubSub => {
                    if let Some(out) = self.pubsub.pop_output(now, &mut self.switcher) {
                        return Some((Features::PubSub, out.into2()));
                    }
                }
                Features::Alias => {
                    if let Some(out) = self.alias.pop_output(now, &mut self.switcher) {
                        return Some((Features::Alias, out.into2()));
                    }
                }
                Features::Socket => {
                    if let Some(out) = self.socket.pop_output(now, &mut self.switcher) {
                        return Some((Features::Socket, out.into2()));
                    }
                }
            }
        }
    }
}

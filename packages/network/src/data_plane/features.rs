use std::fmt::Debug;
use std::net::SocketAddr;

use atm0s_sdn_identity::ConnId;
use sans_io_runtime::{TaskSwitcher, TaskSwitcherBranch, TaskSwitcherChild};

use crate::base::{Buffer, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, TransportMsgHeader};
use crate::features::*;

pub type FeaturesWorkerInput<UserData> = FeatureWorkerInput<UserData, FeaturesControl, FeaturesToWorker<UserData>>;
pub type FeaturesWorkerOutput<UserData> = FeatureWorkerOutput<UserData, FeaturesControl, FeaturesEvent, FeaturesToController>;

pub type Output<UserData> = (Features, FeaturesWorkerOutput<UserData>);

///
/// FeatureWorkerManager is a manager for all features
/// This will take-care of how to route the input to the correct feature
/// With some special event must to broadcast to all features (Tick, Transport Events), it will
/// use a switcher to correctly process one by one
///
pub struct FeatureWorkerManager<UserData> {
    neighbours: TaskSwitcherBranch<neighbours::NeighboursFeatureWorker<UserData>, neighbours::WorkerOutput<UserData>>,
    data: TaskSwitcherBranch<data::DataFeatureWorker<UserData>, data::WorkerOutput<UserData>>,
    router_sync: TaskSwitcherBranch<router_sync::RouterSyncFeatureWorker<UserData>, router_sync::WorkerOutput<UserData>>,
    vpn: TaskSwitcherBranch<vpn::VpnFeatureWorker<UserData>, vpn::WorkerOutput<UserData>>,
    dht_kv: TaskSwitcherBranch<dht_kv::DhtKvFeatureWorker<UserData>, dht_kv::WorkerOutput<UserData>>,
    pubsub: TaskSwitcherBranch<pubsub::PubSubFeatureWorker<UserData>, pubsub::WorkerOutput<UserData>>,
    alias: TaskSwitcherBranch<alias::AliasFeatureWorker<UserData>, alias::WorkerOutput<UserData>>,
    socket: TaskSwitcherBranch<socket::SocketFeatureWorker<UserData>, socket::WorkerOutput<UserData>>,
    switcher: TaskSwitcher,
}

impl<UserData: Eq + Debug + Copy> FeatureWorkerManager<UserData> {
    pub fn new() -> Self {
        Self {
            neighbours: TaskSwitcherBranch::default(Features::Neighbours as usize),
            data: TaskSwitcherBranch::default(Features::Data as usize),
            router_sync: TaskSwitcherBranch::default(Features::RouterSync as usize),
            vpn: TaskSwitcherBranch::default(Features::Vpn as usize),
            dht_kv: TaskSwitcherBranch::default(Features::DhtKv as usize),
            pubsub: TaskSwitcherBranch::default(Features::PubSub as usize),
            alias: TaskSwitcherBranch::default(Features::Alias as usize),
            socket: TaskSwitcherBranch::default(Features::Socket as usize),
            switcher: TaskSwitcher::new(8),
        }
    }

    pub fn on_tick(&mut self, ctx: &mut FeatureWorkerContext, now_ms: u64, tick_count: u64) {
        self.neighbours.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
        self.data.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
        self.router_sync.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
        self.vpn.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
        self.dht_kv.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
        self.pubsub.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
        self.alias.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
        self.socket.input(&mut self.switcher).on_tick(ctx, now_ms, tick_count);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn on_network_raw(&mut self, ctx: &mut FeatureWorkerContext, feature: Features, now_ms: u64, conn: ConnId, remote: SocketAddr, header: TransportMsgHeader, buf: Buffer) {
        match feature {
            Features::Neighbours => self.neighbours.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
            Features::Data => self.data.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
            Features::RouterSync => self.router_sync.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
            Features::Vpn => self.vpn.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
            Features::DhtKv => self.dht_kv.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
            Features::PubSub => self.pubsub.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
            Features::Alias => self.alias.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
            Features::Socket => self.socket.input(&mut self.switcher).on_network_raw(ctx, now_ms, conn, remote, header, buf),
        }
    }

    pub fn on_input(&mut self, ctx: &mut FeatureWorkerContext, feature: Features, now_ms: u64, input: FeaturesWorkerInput<UserData>) {
        match input {
            FeatureWorkerInput::Control(actor, control) => match control {
                FeaturesControl::Neighbours(control) => self.neighbours.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
                FeaturesControl::Data(control) => self.data.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
                FeaturesControl::RouterSync(control) => self.router_sync.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
                FeaturesControl::Vpn(control) => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
                FeaturesControl::DhtKv(control) => self.dht_kv.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
                FeaturesControl::PubSub(control) => self.pubsub.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
                FeaturesControl::Alias(control) => self.alias.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
                FeaturesControl::Socket(control) => self.socket.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Control(actor, control)),
            },
            FeatureWorkerInput::FromController(is_broadcast, to) => match to {
                FeaturesToWorker::Neighbours(to) => self.neighbours.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
                FeaturesToWorker::Data(to) => self.data.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
                FeaturesToWorker::RouterSync(to) => self.router_sync.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
                FeaturesToWorker::Vpn(to) => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
                FeaturesToWorker::DhtKv(to) => self.dht_kv.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
                FeaturesToWorker::PubSub(to) => self.pubsub.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
                FeaturesToWorker::Alias(to) => self.alias.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
                FeaturesToWorker::Socket(to) => self.socket.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)),
            },
            FeatureWorkerInput::Network(..) => {
                panic!("should call above on_network_raw")
            }
            #[cfg(feature = "vpn")]
            FeatureWorkerInput::TunPkt(pkt) => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::TunPkt(pkt)),
            FeatureWorkerInput::Local(header, buf) => match feature {
                Features::Neighbours => self.neighbours.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
                Features::Data => self.data.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
                Features::RouterSync => self.router_sync.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
                Features::Vpn => self.vpn.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
                Features::DhtKv => self.dht_kv.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
                Features::PubSub => self.pubsub.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
                Features::Alias => self.alias.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
                Features::Socket => self.socket.input(&mut self.switcher).on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)),
            },
        }
    }
}

impl<UserData> TaskSwitcherChild<Output<UserData>> for FeatureWorkerManager<UserData> {
    type Time = u64;
    fn pop_output(&mut self, now: u64) -> Option<Output<UserData>> {
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

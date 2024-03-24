use std::net::SocketAddr;

use atm0s_sdn_identity::{ConnId, NodeId};

use crate::base::{FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer};
use crate::features::*;

pub type FeaturesWorkerInput<'a> = FeatureWorkerInput<'a, FeaturesControl, FeaturesToWorker>;
pub type FeaturesWorkerOutput<'a> = FeatureWorkerOutput<'a, FeaturesControl, FeaturesEvent, FeaturesToController>;

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
    vpn: vpn::VpnFeatureWorker,
    dht_kv: dht_kv::DhtKvFeatureWorker,
    pubsub: pubsub::PubSubFeatureWorker,
    last_input_feature: Option<Features>,
    switcher: TasksSwitcher<5>,
}

impl FeatureWorkerManager {
    pub fn new(node: NodeId) -> Self {
        Self {
            neighbours: neighbours::NeighboursFeatureWorker::default(),
            data: data::DataFeatureWorker::default(),
            router_sync: router_sync::RouterSyncFeatureWorker::default(),
            vpn: vpn::VpnFeatureWorker::new(node),
            dht_kv: dht_kv::DhtKvFeatureWorker::default(),
            pubsub: pubsub::PubSubFeatureWorker::new(node),
            last_input_feature: None,
            switcher: TasksSwitcher::default(),
        }
    }

    pub fn on_tick(&mut self, ctx: &mut FeatureWorkerContext, now_ms: u64, tick_count: u64) {
        self.last_input_feature = None;
        self.neighbours.on_tick(ctx, now_ms, tick_count);
        self.data.on_tick(ctx, now_ms, tick_count);
        self.router_sync.on_tick(ctx, now_ms, tick_count);
        self.vpn.on_tick(ctx, now_ms, tick_count);
        self.dht_kv.on_tick(ctx, now_ms, tick_count);
        self.pubsub.on_tick(ctx, now_ms, tick_count);
    }

    pub fn on_network_raw<'a>(
        &mut self,
        ctx: &mut FeatureWorkerContext,
        feature: Features,
        now_ms: u64,
        conn: ConnId,
        remote: SocketAddr,
        header_len: usize,
        buf: GenericBuffer<'a>,
    ) -> Option<FeaturesWorkerOutput<'a>> {
        match feature {
            Features::Neighbours => self.neighbours.on_network_raw(ctx, now_ms, conn, remote, header_len, buf).map(|a| a.into2()),
            Features::Data => self.data.on_network_raw(ctx, now_ms, conn, remote, header_len, buf).map(|a| a.into2()),
            Features::RouterSync => self.router_sync.on_network_raw(ctx, now_ms, conn, remote, header_len, buf).map(|a| a.into2()),
            Features::Vpn => self.vpn.on_network_raw(ctx, now_ms, conn, remote, header_len, buf).map(|a| a.into2()),
            Features::DhtKv => self.dht_kv.on_network_raw(ctx, now_ms, conn, remote, header_len, buf).map(|a| a.into2()),
            Features::PubSub => self.pubsub.on_network_raw(ctx, now_ms, conn, remote, header_len, buf).map(|a| a.into2()),
        }
    }

    pub fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, feature: Features, now_ms: u64, input: FeaturesWorkerInput<'a>) -> Option<FeaturesWorkerOutput<'a>> {
        match input {
            FeatureWorkerInput::Control(service, control) => match control {
                FeaturesControl::Neighbours(control) => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::Data(control) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::RouterSync(control) => self.router_sync.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::Vpn(control) => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::DhtKv(control) => self.dht_kv.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::PubSub(control) => self.pubsub.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
            },
            FeatureWorkerInput::FromController(to) => match to {
                FeaturesToWorker::Neighbours(to) => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
                FeaturesToWorker::Data(to) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
                FeaturesToWorker::RouterSync(to) => self.router_sync.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
                FeaturesToWorker::Vpn(to) => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
                FeaturesToWorker::DhtKv(to) => self.dht_kv.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
                FeaturesToWorker::PubSub(to) => self.pubsub.on_input(ctx, now_ms, FeatureWorkerInput::FromController(to)).map(|a| a.into2()),
            },
            FeatureWorkerInput::Network(_conn, _buf) => {
                panic!("should call above on_network_raw")
            }
            FeatureWorkerInput::TunPkt(pkt) => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::TunPkt(pkt)).map(|a| a.into2()),
            FeatureWorkerInput::Local(buf) => match feature {
                Features::Neighbours => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| a.into2()),
                Features::Data => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| a.into2()),
                Features::RouterSync => self.router_sync.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| a.into2()),
                Features::Vpn => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| a.into2()),
                Features::DhtKv => self.dht_kv.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| a.into2()),
                Features::PubSub => self.pubsub.on_input(ctx, now_ms, FeatureWorkerInput::Local(buf)).map(|a| a.into2()),
            },
        }
    }

    pub fn pop_output(&mut self) -> Option<(Features, FeaturesWorkerOutput<'static>)> {
        if let Some(last_feature) = self.last_input_feature {
            let res = match last_feature {
                Features::Neighbours => self.neighbours.pop_output().map(|a| (Features::Neighbours, a.owned().into2())),
                Features::Data => self.data.pop_output().map(|a| (Features::Data, a.owned().into2())),
                Features::RouterSync => self.router_sync.pop_output().map(|a| (Features::RouterSync, a.owned().into2())),
                Features::Vpn => self.vpn.pop_output().map(|a| (Features::Vpn, a.owned().into2())),
                Features::DhtKv => self.dht_kv.pop_output().map(|a| (Features::DhtKv, a.owned().into2())),
                Features::PubSub => self.pubsub.pop_output().map(|a| (Features::PubSub, a.owned().into2())),
            };

            if res.is_none() {
                self.last_input_feature = None;
            }

            res
        } else {
            loop {
                let s = &mut self.switcher;
                match (s.current()? as u8).try_into().ok()? {
                    Features::Neighbours => {
                        if let Some(out) = s.process(self.neighbours.pop_output()) {
                            return Some((Features::Neighbours, out.owned().into2()));
                        }
                    }
                    Features::Data => {
                        if let Some(out) = s.process(self.data.pop_output()) {
                            return Some((Features::Data, out.owned().into2()));
                        }
                    }
                    Features::RouterSync => {
                        if let Some(out) = s.process(self.router_sync.pop_output()) {
                            return Some((Features::RouterSync, out.owned().into2()));
                        }
                    }
                    Features::Vpn => {
                        if let Some(out) = s.process(self.vpn.pop_output()) {
                            return Some((Features::Vpn, out.owned().into2()));
                        }
                    }
                    Features::DhtKv => {
                        if let Some(out) = s.process(self.dht_kv.pop_output()) {
                            return Some((Features::DhtKv, out.owned().into2()));
                        }
                    }
                    Features::PubSub => {
                        if let Some(out) = s.process(self.pubsub.pop_output()) {
                            return Some((Features::PubSub, out.owned().into2()));
                        }
                    }
                }
            }
        }
    }
}

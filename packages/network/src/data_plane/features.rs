use std::net::SocketAddr;

use atm0s_sdn_identity::ConnId;
use sans_io_runtime::TaskSwitcher;

use crate::base::{FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer, TransportMsgHeader};
use crate::features::*;

pub type FeaturesWorkerInput<'a> = FeatureWorkerInput<'a, FeaturesControl, FeaturesToWorker>;
pub type FeaturesWorkerOutput<'a> = FeatureWorkerOutput<'a, FeaturesControl, FeaturesEvent, FeaturesToController>;

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
    alias: alias::AliasFeatureWorker,
    socket: socket::SocketFeatureWorker,
    switcher: TaskSwitcher,
}

impl FeatureWorkerManager {
    pub fn new() -> Self {
        Self {
            neighbours: neighbours::NeighboursFeatureWorker::default(),
            data: data::DataFeatureWorker::default(),
            router_sync: router_sync::RouterSyncFeatureWorker::default(),
            vpn: vpn::VpnFeatureWorker,
            dht_kv: dht_kv::DhtKvFeatureWorker::default(),
            pubsub: pubsub::PubSubFeatureWorker::new(),
            alias: alias::AliasFeatureWorker::default(),
            socket: socket::SocketFeatureWorker::default(),
            switcher: TaskSwitcher::new(8),
        }
    }

    pub fn on_tick(&mut self, ctx: &mut FeatureWorkerContext, now_ms: u64, tick_count: u64) {
        self.switcher.queue_flag_all();
        self.neighbours.on_tick(ctx, now_ms, tick_count);
        self.data.on_tick(ctx, now_ms, tick_count);
        self.router_sync.on_tick(ctx, now_ms, tick_count);
        self.vpn.on_tick(ctx, now_ms, tick_count);
        self.dht_kv.on_tick(ctx, now_ms, tick_count);
        self.pubsub.on_tick(ctx, now_ms, tick_count);
        self.alias.on_tick(ctx, now_ms, tick_count);
        self.socket.on_tick(ctx, now_ms, tick_count);
    }

    pub fn on_network_raw<'a>(
        &mut self,
        ctx: &mut FeatureWorkerContext,
        feature: Features,
        now_ms: u64,
        conn: ConnId,
        remote: SocketAddr,
        header: TransportMsgHeader,
        buf: GenericBuffer<'a>,
    ) -> Option<FeaturesWorkerOutput<'a>> {
        let out = match feature {
            Features::Neighbours => self.neighbours.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
            Features::Data => self.data.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
            Features::RouterSync => self.router_sync.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
            Features::Vpn => self.vpn.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
            Features::DhtKv => self.dht_kv.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
            Features::PubSub => self.pubsub.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
            Features::Alias => self.alias.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
            Features::Socket => self.socket.on_network_raw(ctx, now_ms, conn, remote, header, buf).map(|a| a.into2()),
        };
        if out.is_some() {
            self.switcher.queue_flag_task(feature as usize);
        }
        out
    }

    pub fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, feature: Features, now_ms: u64, input: FeaturesWorkerInput<'a>) -> Option<FeaturesWorkerOutput<'a>> {
        let out = match input {
            FeatureWorkerInput::Control(service, control) => match control {
                FeaturesControl::Neighbours(control) => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::Data(control) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::RouterSync(control) => self.router_sync.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::Vpn(control) => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::DhtKv(control) => self.dht_kv.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::PubSub(control) => self.pubsub.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::Alias(control) => self.alias.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
                FeaturesControl::Socket(control) => self.socket.on_input(ctx, now_ms, FeatureWorkerInput::Control(service, control)).map(|a| a.into2()),
            },
            FeatureWorkerInput::FromController(is_broadcast, to) => match to {
                FeaturesToWorker::Neighbours(to) => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
                FeaturesToWorker::Data(to) => self.data.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
                FeaturesToWorker::RouterSync(to) => self.router_sync.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
                FeaturesToWorker::Vpn(to) => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
                FeaturesToWorker::DhtKv(to) => self.dht_kv.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
                FeaturesToWorker::PubSub(to) => self.pubsub.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
                FeaturesToWorker::Alias(to) => self.alias.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
                FeaturesToWorker::Socket(to) => self.socket.on_input(ctx, now_ms, FeatureWorkerInput::FromController(is_broadcast, to)).map(|a| a.into2()),
            },
            FeatureWorkerInput::Network(_conn, _header, _buf) => {
                panic!("should call above on_network_raw")
            }
            FeatureWorkerInput::TunPkt(pkt) => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::TunPkt(pkt)).map(|a| a.into2()),
            FeatureWorkerInput::Local(header, buf) => match feature {
                Features::Neighbours => self.neighbours.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
                Features::Data => self.data.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
                Features::RouterSync => self.router_sync.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
                Features::Vpn => self.vpn.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
                Features::DhtKv => self.dht_kv.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
                Features::PubSub => self.pubsub.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
                Features::Alias => self.alias.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
                Features::Socket => self.socket.on_input(ctx, now_ms, FeatureWorkerInput::Local(header, buf)).map(|a| a.into2()),
            },
        };
        if out.is_some() {
            self.switcher.queue_flag_task(feature as usize);
        }
        out
    }

    pub fn pop_output(&mut self, ctx: &mut FeatureWorkerContext) -> Option<(Features, FeaturesWorkerOutput<'static>)> {
        loop {
            let s = &mut self.switcher;
            match (s.queue_current()? as u8).try_into().ok()? {
                Features::Neighbours => {
                    if let Some(out) = s.queue_process(self.neighbours.pop_output(ctx)) {
                        return Some((Features::Neighbours, out.owned().into2()));
                    }
                }
                Features::Data => {
                    if let Some(out) = s.queue_process(self.data.pop_output(ctx)) {
                        return Some((Features::Data, out.owned().into2()));
                    }
                }
                Features::RouterSync => {
                    if let Some(out) = s.queue_process(self.router_sync.pop_output(ctx)) {
                        return Some((Features::RouterSync, out.owned().into2()));
                    }
                }
                Features::Vpn => {
                    if let Some(out) = s.queue_process(self.vpn.pop_output(ctx)) {
                        return Some((Features::Vpn, out.owned().into2()));
                    }
                }
                Features::DhtKv => {
                    if let Some(out) = s.queue_process(self.dht_kv.pop_output(ctx)) {
                        return Some((Features::DhtKv, out.owned().into2()));
                    }
                }
                Features::PubSub => {
                    if let Some(out) = s.queue_process(self.pubsub.pop_output(ctx)) {
                        return Some((Features::PubSub, out.owned().into2()));
                    }
                }
                Features::Alias => {
                    if let Some(out) = s.queue_process(self.alias.pop_output(ctx)) {
                        return Some((Features::Alias, out.owned().into2()));
                    }
                }
                Features::Socket => {
                    if let Some(out) = s.queue_process(self.socket.pop_output(ctx)) {
                        return Some((Features::Socket, out.owned().into2()));
                    }
                }
            }
        }
    }
}

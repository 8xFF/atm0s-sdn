use std::marker::PhantomData;

#[cfg(feature = "vpn")]
use crate::base::TransportMsg;
#[cfg(feature = "vpn")]
use atm0s_sdn_identity::{NodeId, NodeIdType};
#[cfg(feature = "vpn")]
use atm0s_sdn_router::{RouteAction, RouteRule, RouterTable};
use derivative::Derivative;
use sans_io_runtime::{collections::DynamicDeque, TaskSwitcherChild};

use crate::base::{Buffer, Feature, FeatureContext, FeatureInput, FeatureOutput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput};

pub const FEATURE_ID: u8 = 3;
pub const FEATURE_NAME: &str = "vpn";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

pub type Output<UserData> = FeatureOutput<UserData, Event, ToWorker>;
pub type WorkerOutput<UserData> = FeatureWorkerOutput<UserData, Control, Event, ToController>;

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct VpnFeature<UserData> {
    _tmp: PhantomData<UserData>,
    shutdown: bool,
}

impl<UserData> Feature<UserData, Control, Event, ToController, ToWorker> for VpnFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, _input: crate::base::FeatureSharedInput) {}

    fn on_input(&mut self, _ctx: &FeatureContext, _now_ms: u64, _input: FeatureInput<'_, UserData, Control, ToController>) {}

    fn on_shutdown(&mut self, _ctx: &FeatureContext, _now: u64) {
        self.shutdown = true;
    }
}

impl<UserData> TaskSwitcherChild<Output<UserData>> for VpnFeature<UserData> {
    type Time = u64;

    fn is_empty(&self) -> bool {
        self.shutdown
    }

    fn empty_event(&self) -> Output<UserData> {
        Output::OnResourceEmpty
    }

    fn pop_output(&mut self, _now: u64) -> Option<Output<UserData>> {
        None
    }
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
pub struct VpnFeatureWorker<UserData> {
    queue: DynamicDeque<WorkerOutput<UserData>, 16>,
    shutdown: bool,
}

impl<UserData> VpnFeatureWorker<UserData> {
    #[cfg(feature = "vpn")]
    fn process_tun(&mut self, ctx: &FeatureWorkerContext, mut pkt: Buffer) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let to_ip = &pkt[20..24];
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let to_ip = &pkt[16..20];
        let dest = NodeId::build(ctx.node_id.geo1(), ctx.node_id.geo2(), ctx.node_id.group(), to_ip[3]);
        if dest == ctx.node_id {
            //This is for current node, just echo back
            rewrite_tun_pkt(&mut pkt);
            self.queue.push_back(FeatureWorkerOutput::TunPkt(pkt));
        } else if let RouteAction::Next(remote) = ctx.router.path_to_node(dest) {
            //TODO decrease TTL
            //TODO how to avoid copy data here
            self.queue
                .push_back(FeatureWorkerOutput::RawDirect2(remote, TransportMsg::build(FEATURE_ID, 0, RouteRule::ToNode(dest), &pkt).take()));
        }
    }

    fn process_udp(&mut self, _ctx: &FeatureWorkerContext, pkt: Buffer) {
        #[cfg(feature = "vpn")]
        {
            self.queue.push_back(FeatureWorkerOutput::TunPkt(pkt));
        }
    }
}

impl<UserData> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for VpnFeatureWorker<UserData> {
    fn on_input(&mut self, ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<UserData, Control, ToWorker>) {
        match input {
            #[cfg(feature = "vpn")]
            FeatureWorkerInput::TunPkt(pkt) => self.process_tun(ctx, pkt),
            FeatureWorkerInput::Network(_conn, _header, pkt) => self.process_udp(ctx, pkt),
            _ => {}
        }
    }

    fn on_shutdown(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64) {
        log::info!("[VpnFeatureWorker] Shutdown");
        self.shutdown = true;
    }
}

impl<UserData> TaskSwitcherChild<WorkerOutput<UserData>> for VpnFeatureWorker<UserData> {
    type Time = u64;

    fn is_empty(&self) -> bool {
        self.shutdown && self.queue.is_empty()
    }

    fn empty_event(&self) -> WorkerOutput<UserData> {
        WorkerOutput::OnResourceEmpty
    }

    fn pop_output(&mut self, _now: u64) -> Option<WorkerOutput<UserData>> {
        self.queue.pop_front()
    }
}

#[cfg(feature = "vpn")]
fn rewrite_tun_pkt(payload: &mut [u8]) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        payload[2] = 0;
        payload[3] = 2;
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        payload[2] = 8;
        payload[3] = 0;
    }
}

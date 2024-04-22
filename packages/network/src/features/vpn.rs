use std::marker::PhantomData;

#[cfg(feature = "vpn")]
use crate::base::{BufferMut, TransportMsg};
#[cfg(feature = "vpn")]
use atm0s_sdn_identity::{NodeId, NodeIdType};
#[cfg(feature = "vpn")]
use atm0s_sdn_router::{RouteAction, RouteRule, RouterTable};
use derivative::Derivative;

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

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct VpnFeature<UserData> {
    _tmp: PhantomData<UserData>,
}

impl<UserData> Feature<UserData, Control, Event, ToController, ToWorker> for VpnFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, _input: crate::base::FeatureSharedInput) {}

    fn on_input<'a>(&mut self, _ctx: &FeatureContext, _now_ms: u64, _input: FeatureInput<'a, UserData, Control, ToController>) {}

    fn pop_output<'a>(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<UserData, Event, ToWorker>> {
        None
    }
}

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct VpnFeatureWorker<UserData>(PhantomData<UserData>);

impl<UserData> VpnFeatureWorker<UserData> {
    #[cfg(feature = "vpn")]
    fn process_tun<'a>(&mut self, ctx: &FeatureWorkerContext, mut pkt: BufferMut<'a>) -> Option<FeatureWorkerOutput<'a, UserData, Control, Event, ToController>> {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let to_ip = &pkt[20..24];
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let to_ip = &pkt[16..20];
        let dest = NodeId::build(ctx.node_id.geo1(), ctx.node_id.geo2(), ctx.node_id.group(), to_ip[3]);
        if dest == ctx.node_id {
            //This is for me, just echo back
            rewrite_tun_pkt(&mut pkt);
            Some(FeatureWorkerOutput::TunPkt(pkt.freeze()))
        } else {
            match ctx.router.path_to_node(dest) {
                RouteAction::Next(remote) => {
                    //TODO decrease TTL
                    //TODO how to avoid copy data here
                    Some(FeatureWorkerOutput::RawDirect2(remote, TransportMsg::build(FEATURE_ID, 0, RouteRule::ToNode(dest), &pkt).take().into()))
                }
                _ => None,
            }
        }
    }

    fn process_udp<'a>(&self, _ctx: &FeatureWorkerContext, pkt: Buffer<'a>) -> Option<FeatureWorkerOutput<'a, UserData, Control, Event, ToController>> {
        #[cfg(feature = "vpn")]
        {
            Some(FeatureWorkerOutput::TunPkt(pkt))
        }
        #[cfg(not(feature = "vpn"))]
        None
    }
}

impl<UserData> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for VpnFeatureWorker<UserData> {
    fn on_input<'a>(
        &mut self,
        ctx: &mut FeatureWorkerContext,
        _now: u64,
        input: FeatureWorkerInput<'a, UserData, Control, ToWorker>,
    ) -> Option<FeatureWorkerOutput<'a, UserData, Control, Event, ToController>> {
        match input {
            #[cfg(feature = "vpn")]
            FeatureWorkerInput::TunPkt(pkt) => self.process_tun(ctx, pkt),
            FeatureWorkerInput::Network(_conn, _header, pkt) => self.process_udp(ctx, pkt),
            _ => None,
        }
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

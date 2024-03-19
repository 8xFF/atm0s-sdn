use atm0s_sdn_identity::{NodeId, NodeIdType};
use atm0s_sdn_router::{RouteAction, RouteRule, RouterTable};

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer, GenericBufferMut, TransportMsg};

pub const FEATURE_ID: u8 = 3;
pub const FEATURE_NAME: &str = "vpn";

#[derive(Debug, Clone)]
pub enum Control {}

#[derive(Debug, Clone)]
pub enum Event {}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Default)]
pub struct VpnFeature {}

impl Feature<Control, Event, ToController, ToWorker> for VpnFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, _now: u64, _input: crate::base::FeatureSharedInput) {}

    fn on_input<'a>(&mut self, _now_ms: u64, _input: FeatureInput<'a, Control, ToController>) {}

    fn pop_output<'a>(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        None
    }
}

pub struct VpnFeatureWorker {
    node_id: NodeId,
}

impl VpnFeatureWorker {
    pub fn new(node_id: NodeId) -> Self {
        Self { node_id }
    }

    fn process_tun<'a>(&mut self, ctx: &FeatureWorkerContext, mut pkt: GenericBufferMut<'a>) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let to_ip = &pkt[20..24];
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let to_ip = &pkt[16..20];
        let dest = NodeId::build(self.node_id.geo1(), self.node_id.geo2(), self.node_id.group(), to_ip[3]);
        if dest == self.node_id {
            //This is for me, just echo back
            rewrite_tun_pkt(&mut pkt);
            Some(FeatureWorkerOutput::TunPkt(pkt.to_readonly()))
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

    fn process_udp<'a>(&self, _ctx: &FeatureWorkerContext, pkt: GenericBuffer<'a>) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        Some(FeatureWorkerOutput::TunPkt(pkt))
    }
}

impl FeatureWorker<Control, Event, ToController, ToWorker> for VpnFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<'a, Control, ToWorker>) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        match input {
            FeatureWorkerInput::TunPkt(pkt) => self.process_tun(ctx, pkt),
            FeatureWorkerInput::Network(_conn, pkt) => self.process_udp(ctx, pkt),
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

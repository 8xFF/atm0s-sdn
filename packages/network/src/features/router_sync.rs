use atm0s_sdn_router::RouteRule;

use crate::base::{ConnectionEvent, Feature, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker};

pub const FEATURE_ID: u8 = 2;
pub const FEATURE_NAME: &str = "router_sync";

#[derive(Debug, Clone)]
pub enum Control {
    Send(RouteRule, Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum Event {
    Data(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Default)]
pub struct RouterSyncFeature {}

impl Feature<Control, Event, ToController, ToWorker> for RouterSyncFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_input<'a>(&mut self, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Shared(FeatureSharedInput::Connection(event)) => match event {
                ConnectionEvent::Connected(ctx) => {
                    log::info!("Connection {} connected", ctx.remote);
                }
                ConnectionEvent::Stats(ctx, stats) => {
                    log::info!("Connection {} stats rtt_ms {}", ctx.remote, stats.rtt_ms);
                }
                ConnectionEvent::Disconnected(ctx) => {
                    log::info!("Connection {} disconnected", ctx.remote);
                }
            },
            _ => {}
        }
    }

    fn pop_output(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        None
    }
}

#[derive(Default)]
pub struct RouterSyncFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for RouterSyncFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }
}

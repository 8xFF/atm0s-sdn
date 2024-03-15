use std::net::SocketAddr;

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::shadow::ShadowRouter;

use crate::base::FeatureWorkerContext;

use self::{features::FeatureWorkerManager, services::ServiceWorkerManager};

mod features;
mod services;

pub enum Input<'a> {
    NetworkEvent(SocketAddr, &'a [u8]),
}

pub enum Output<'a> {
    NetworkEvent(SocketAddr, &'a [u8]),
    NetworkEvents(Vec<SocketAddr>, &'a [u8]),
}

pub struct DataPlane<TC, TW> {
    ctx: FeatureWorkerContext,
    features: FeatureWorkerManager,
    services: ServiceWorkerManager<TC, TW>,
}

impl<TC, TW> DataPlane<TC, TW> {
    pub fn new(node_id: NodeId) -> Self {
        // Self {
        //     ctx: FeatureWorkerContext {
        //         router: ShadowRouter::new(node_id),
        //     },
        // }
        todo!()
    }

    pub fn on_tick<'a>(&mut self, now_ms: u64) {
        self.features.on_tick(&mut self.ctx, now_ms);
        self.services.on_tick(now_ms);
    }

    pub fn on_event<'a>(&mut self, now_ms: u64, event: Input) -> Option<Output<'a>> {
        None
    }

    pub fn pop_output<'a>(&mut self, now_ms: u64) -> Option<Output<'a>> {
        None
    }
}

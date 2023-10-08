use std::sync::{atomic::AtomicU64, Arc};

use bluesea_identity::NodeId;
use parking_lot::RwLock;

use crate::{
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{local::LocalRelay, logic::PubsubRelayLogic, remote::RemoteRelay, ChannelIdentify, ChannelUuid},
};

use self::{consumer::Consumer, publisher::Publisher};

pub(crate) mod consumer;
pub(crate) mod publisher;

pub struct PubsubSdk<BE, HE> {
    node_id: NodeId,
    pub_uuid_seed: Arc<AtomicU64>,
    sub_uuid_seed: Arc<AtomicU64>,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    remote: Arc<RwLock<RemoteRelay<BE, HE>>>,
    local: Arc<RwLock<LocalRelay>>,
}

impl<BE, HE> Clone for PubsubSdk<BE, HE> {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id,
            pub_uuid_seed: self.pub_uuid_seed.clone(),
            sub_uuid_seed: self.sub_uuid_seed.clone(),
            logic: self.logic.clone(),
            remote: self.remote.clone(),
            local: self.local.clone(),
        }
    }
}

impl<BE, HE> PubsubSdk<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(node_id: NodeId, logic: Arc<RwLock<PubsubRelayLogic>>, remote: Arc<RwLock<RemoteRelay<BE, HE>>>, local: Arc<RwLock<LocalRelay>>) -> Self {
        Self {
            node_id,
            pub_uuid_seed: Arc::new(AtomicU64::new(0)),
            sub_uuid_seed: Arc::new(AtomicU64::new(0)),
            local,
            remote,
            logic,
        }
    }

    pub fn create_publisher(&self, channel: ChannelUuid) -> Publisher<BE, HE> {
        let uuid = self.pub_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Publisher::<BE, HE>::new(uuid, ChannelIdentify::new(channel, self.node_id), self.logic.clone(), self.remote.clone(), self.local.clone())
    }

    pub fn create_consumer(&self, channel: ChannelIdentify, max_queue_size: Option<usize>) -> Consumer {
        let uuid = self.sub_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Consumer::new(uuid, channel, self.logic.clone(), self.local.clone(), max_queue_size.unwrap_or(100))
    }
}

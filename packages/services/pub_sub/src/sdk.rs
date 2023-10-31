use std::sync::{atomic::AtomicU64, Arc};

use async_std::channel::Sender;
use bluesea_identity::NodeId;
use bytes::Bytes;
use parking_lot::RwLock;

use crate::{
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{feedback::Feedback, local::LocalRelay, logic::PubsubRelayLogic, remote::RemoteRelay, source_binding::SourceBinding, ChannelIdentify, ChannelUuid, LocalSubId},
};

use self::{consumer::Consumer, consumer_raw::ConsumerRaw, consumer_single::ConsumerSingle, publisher::Publisher, publisher_raw::PublisherRaw};

pub(crate) mod consumer;
pub(crate) mod consumer_raw;
pub(crate) mod consumer_single;
pub(crate) mod publisher;
pub(crate) mod publisher_raw;

pub struct PubsubSdk<BE, HE> {
    node_id: NodeId,
    pub_uuid_seed: Arc<AtomicU64>,
    sub_uuid_seed: Arc<AtomicU64>,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    remote: Arc<RwLock<RemoteRelay<BE, HE>>>,
    local: Arc<RwLock<LocalRelay>>,
    source_binding: Arc<RwLock<SourceBinding>>,
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
            source_binding: self.source_binding.clone(),
        }
    }
}

impl<BE, HE> PubsubSdk<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(node_id: NodeId, logic: Arc<RwLock<PubsubRelayLogic>>, remote: Arc<RwLock<RemoteRelay<BE, HE>>>, local: Arc<RwLock<LocalRelay>>, source_binding: Arc<RwLock<SourceBinding>>) -> Self {
        Self {
            node_id,
            pub_uuid_seed: Arc::new(AtomicU64::new(0)),
            sub_uuid_seed: Arc::new(AtomicU64::new(0)),
            local,
            remote,
            logic,
            source_binding,
        }
    }

    pub fn create_publisher(&self, channel: ChannelUuid) -> Publisher<BE, HE> {
        let uuid = self.pub_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Publisher::<BE, HE>::new(uuid, ChannelIdentify::new(channel, self.node_id), self.logic.clone(), self.remote.clone(), self.local.clone())
    }

    pub fn create_publisher_raw(&self, channel: ChannelUuid, fb_tx: async_std::channel::Sender<Feedback>) -> PublisherRaw<BE, HE> {
        let uuid = self.pub_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        PublisherRaw::<BE, HE>::new(uuid, ChannelIdentify::new(channel, self.node_id), self.logic.clone(), self.remote.clone(), self.local.clone(), fb_tx)
    }

    pub fn create_consumer_single(&self, channel: ChannelIdentify, max_queue_size: Option<usize>) -> ConsumerSingle {
        let uuid = self.sub_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ConsumerSingle::new(uuid, channel, self.logic.clone(), self.local.clone(), max_queue_size.unwrap_or(100))
    }

    pub fn create_consumer_raw(&self, channel: ChannelUuid, tx: Sender<(LocalSubId, NodeId, ChannelUuid, Bytes)>) -> ConsumerRaw {
        let uuid = self.sub_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ConsumerRaw::new(uuid, channel, self.logic.clone(), self.local.clone(), self.source_binding.clone(), tx)
    }

    pub fn create_consumer(&self, channel: ChannelUuid, max_queue_size: Option<usize>) -> Consumer {
        let uuid = self.sub_uuid_seed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Consumer::new(uuid, channel, self.logic.clone(), self.local.clone(), self.source_binding.clone(), max_queue_size.unwrap_or(100))
    }
}

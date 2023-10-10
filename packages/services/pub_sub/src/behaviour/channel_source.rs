use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use async_std::channel::Sender;
use bluesea_identity::NodeId;
use key_value::{KeyId, KeySource, KeyValueSdk, KeyVersion, SubKeyId, ValueType};
use parking_lot::Mutex;

pub type SourceMapEvent = (KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource);

pub trait ChannelSourceHashmap: Send + Sync {
    fn add(&self, key: u64);
    fn remove(&self, key: u64);
    fn subscribe(&self, key: u64, tx: Sender<SourceMapEvent>);
    fn unsubscribe(&self, key: u64);
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChannelSourceHashmapMockOutput {
    Add(u64),
    Remove(u64),
    Subscribe(u64),
    Unsubscribe(u64),
}

pub struct ChannelSourceHashmapMock {
    events: Arc<Mutex<VecDeque<ChannelSourceHashmapMockOutput>>>,
    hashmap: Arc<Mutex<HashMap<u64, Sender<SourceMapEvent>>>>,
}

impl ChannelSourceHashmapMock {
    pub fn new() -> (Self, Arc<Mutex<VecDeque<ChannelSourceHashmapMockOutput>>>) {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        (
            Self {
                events: events.clone(),
                hashmap: Default::default(),
            },
            events,
        )
    }
}

impl ChannelSourceHashmap for ChannelSourceHashmapMock {
    fn subscribe(&self, key: u64, tx: Sender<SourceMapEvent>) {
        self.events.lock().push_back(ChannelSourceHashmapMockOutput::Subscribe(key));
        self.hashmap.lock().insert(key, tx);
    }

    fn unsubscribe(&self, key: u64) {
        self.events.lock().push_back(ChannelSourceHashmapMockOutput::Unsubscribe(key));
        self.hashmap.lock().remove(&key);
    }

    fn add(&self, key: u64) {
        self.events.lock().push_back(ChannelSourceHashmapMockOutput::Add(key));
    }

    fn remove(&self, key: u64) {
        self.events.lock().push_back(ChannelSourceHashmapMockOutput::Remove(key));
    }
}

const HSUB_UUID: u64 = 8989933434898989;

pub struct ChannelSourceHashmapReal {
    node_id: NodeId,
    sdk: KeyValueSdk,
}

impl ChannelSourceHashmapReal {
    pub fn new(sdk: KeyValueSdk, node_id: NodeId) -> Self {
        Self { node_id, sdk }
    }
}

impl ChannelSourceHashmap for ChannelSourceHashmapReal {
    fn add(&self, key: u64) {
        self.sdk.hset(key, self.node_id as u64, vec![], Some(30000));
    }

    fn remove(&self, key: u64) {
        self.sdk.hdel(key, self.node_id as u64);
    }

    fn subscribe(&self, key: u64, tx: Sender<SourceMapEvent>) {
        self.sdk.hsubscribe_raw(key, HSUB_UUID, Some(30000), tx);
    }

    fn unsubscribe(&self, key: u64) {
        self.sdk.hunsubscribe_raw(key, HSUB_UUID);
    }
}

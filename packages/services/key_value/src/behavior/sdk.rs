use std::{collections::VecDeque, sync::Arc};

use async_std::channel::Sender;
use parking_lot::RwLock;
use utils::awaker::Awaker;

use crate::{ExternalControl, KeyId, KeySource, KeyValueSdkEvent, KeyVersion, SubKeyId, ValueType};

use super::{hashmap_local::HashmapKeyValueGetError, simple_local::SimpleKeyValueGetError};

mod pub_sub;

pub type SimpleKeyValueSubscriber = pub_sub::Subscriber<u64, (KeyId, Option<ValueType>, KeyVersion, KeySource)>;
pub type HashmapKeyValueSubscriber = pub_sub::Subscriber<u64, (KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource)>;

#[derive(Clone)]
pub struct KeyValueSdk {
    awaker: Arc<RwLock<Option<Arc<dyn Awaker>>>>,
    simple_publisher: Arc<pub_sub::PublisherManager<u64, (KeyId, Option<ValueType>, KeyVersion, KeySource)>>,
    hashmap_publisher: Arc<pub_sub::PublisherManager<u64, (KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource)>>,
    actions: Arc<RwLock<VecDeque<crate::KeyValueSdkEvent>>>,
}

impl KeyValueSdk {
    pub fn new() -> Self {
        Self {
            awaker: Arc::new(RwLock::new(None)),
            simple_publisher: Arc::new(pub_sub::PublisherManager::new()),
            hashmap_publisher: Arc::new(pub_sub::PublisherManager::new()),
            actions: Arc::new(RwLock::new(VecDeque::new())),
        }
    }

    pub fn set(&self, key: KeyId, value: Vec<u8>, ex: Option<u64>) {
        self.actions.write().push_back(crate::KeyValueSdkEvent::Set(key, value, ex));
        self.awaker.read().as_ref().unwrap().notify();
    }

    pub async fn get(&self, key: KeyId, timeout_ms: u64) -> Result<Option<(ValueType, KeyVersion, KeySource)>, SimpleKeyValueGetError> {
        todo!()
    }

    pub fn del(&self, key: KeyId) {
        self.actions.write().push_back(crate::KeyValueSdkEvent::Del(key));
        self.awaker.read().as_ref().unwrap().notify();
    }

    pub fn subscribe(&self, key: KeyId, ex: Option<u64>) -> SimpleKeyValueSubscriber {
        let actions = self.actions.clone();
        let awaker = self.awaker.clone();
        let (subscriber, is_new) = self.simple_publisher.subscribe(
            key,
            Box::new(move || {
                actions.write().push_back(crate::KeyValueSdkEvent::Unsub(0, key));
                awaker.read().as_ref().unwrap().notify();
            }),
        );
        if is_new {
            self.actions.write().push_back(crate::KeyValueSdkEvent::Sub(0, key, ex));
            self.awaker.read().as_ref().unwrap().notify();
        }

        subscriber
    }

    pub fn hset(&self, key: KeyId, sub_key: SubKeyId, value: Vec<u8>, ex: Option<u64>) {
        self.actions.write().push_back(crate::KeyValueSdkEvent::SetH(key, sub_key, value, ex));
        self.awaker.read().as_ref().unwrap().notify();
    }

    pub async fn hget(&self, key: KeyId, timeout_ms: u64) -> Result<Option<Vec<(SubKeyId, ValueType, KeyVersion, KeySource)>>, HashmapKeyValueGetError> {
        todo!()
    }

    pub fn hdel(&self, key: KeyId, sub_key: SubKeyId) {
        self.actions.write().push_back(crate::KeyValueSdkEvent::DelH(key, sub_key));
        self.awaker.read().as_ref().unwrap().notify();
    }

    pub fn hsubscribe(&self, key: u64, ex: Option<u64>) -> HashmapKeyValueSubscriber {
        let actions = self.actions.clone();
        let awaker = self.awaker.clone();
        let (subscriber, is_new) = self.hashmap_publisher.subscribe(
            key,
            Box::new(move || {
                actions.write().push_back(crate::KeyValueSdkEvent::UnsubH(0, key));
                awaker.read().as_ref().unwrap().notify();
            }),
        );
        if is_new {
            self.actions.write().push_back(crate::KeyValueSdkEvent::SubH(0, key, ex));
            self.awaker.read().as_ref().unwrap().notify();
        }

        subscriber
    }

    pub fn hsubscribe_raw(&self, key: u64, uuid: u64, ex: Option<u64>, tx: Sender<(KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource)>) {
        if self.hashmap_publisher.sub_raw(key, uuid, tx) {
            self.actions.write().push_back(crate::KeyValueSdkEvent::SubH(0, key, ex));
            self.awaker.read().as_ref().unwrap().notify();
        }
    }

    pub fn hunsubscribe_raw(&self, key: u64, uuid: u64) {
        if self.hashmap_publisher.unsub_raw(key, uuid) {
            self.actions.write().push_back(crate::KeyValueSdkEvent::UnsubH(0, key));
            self.awaker.read().as_ref().unwrap().notify();
        }
    }
}

impl ExternalControl for KeyValueSdk {
    fn set_awaker(&self, awaker: Arc<dyn Awaker>) {
        self.awaker.write().replace(awaker);
    }

    fn on_event(&self, event: KeyValueSdkEvent) {
        match event {
            KeyValueSdkEvent::OnKeyChanged(_uuid, key, value, version, source) => {
                self.simple_publisher.publish(key, (key, value, version, source));
            }
            KeyValueSdkEvent::OnKeyHChanged(_uuid, key, sub_key, value, version, source) => {
                self.hashmap_publisher.publish(key, (key, sub_key, value, version, source));
            }
            _ => {}
        }
    }

    fn pop_action(&self) -> Option<KeyValueSdkEvent> {
        self.actions.write().pop_front()
    }
}

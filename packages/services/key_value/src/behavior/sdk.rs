use std::sync::Arc;

use async_std::channel::Sender;
use parking_lot::RwLock;
use utils::error_handle::ErrorUtils;

use crate::{KeyId, KeySource, KeyVersion, SubKeyId, ValueType};

use super::{
    hashmap_local::{HashmapKeyValueGetError, HashmapLocalStorage},
    simple_local::{SimpleKeyValueGetError, SimpleLocalStorage},
};

mod pub_sub;

pub type SimpleKeyValueSubscriber = pub_sub::Subscriber<u64, (KeyId, Option<ValueType>, KeyVersion, KeySource)>;
pub type HashmapKeyValueSubscriber = pub_sub::Subscriber<u64, (KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource)>;

#[derive(Clone)]
pub struct KeyValueSdk {
    simple_local: Arc<RwLock<SimpleLocalStorage>>,
    hashmap_local: Arc<RwLock<HashmapLocalStorage>>,
    simple_publisher: Arc<pub_sub::PublisherManager<u64, (KeyId, Option<ValueType>, KeyVersion, KeySource)>>,
    hashmap_publisher: Arc<pub_sub::PublisherManager<u64, (KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource)>>,
}

impl KeyValueSdk {
    pub fn new(simple_local: Arc<RwLock<SimpleLocalStorage>>, hashmap_local: Arc<RwLock<HashmapLocalStorage>>) -> Self {
        Self {
            simple_local,
            hashmap_local,
            simple_publisher: Arc::new(pub_sub::PublisherManager::new()),
            hashmap_publisher: Arc::new(pub_sub::PublisherManager::new()),
        }
    }

    pub fn set(&self, key: KeyId, value: Vec<u8>, ex: Option<u64>) {
        self.simple_local.write().set(key, value, ex);
    }

    pub async fn get(&self, key: KeyId, timeout_ms: u64) -> Result<Option<(ValueType, KeyVersion, KeySource)>, SimpleKeyValueGetError> {
        let (tx, rx) = async_std::channel::bounded(1);
        let handler = move |res: Result<Option<(ValueType, KeyVersion, KeySource)>, SimpleKeyValueGetError>| {
            tx.try_send(res).print_error("Should send result");
        };
        self.simple_local.write().get(key, Box::new(handler), timeout_ms);
        rx.recv().await.unwrap_or(Err(SimpleKeyValueGetError::Timeout))
    }

    pub fn del(&self, key: KeyId) {
        self.simple_local.write().del(key);
    }

    pub fn subscribe(&self, key: KeyId, ex: Option<u64>) -> SimpleKeyValueSubscriber {
        let local = self.simple_local.clone();
        let (subscriber, is_new) = self.simple_publisher.subscribe(
            key,
            Box::new(move || {
                local.write().unsubscribe(key);
            }),
        );
        if is_new {
            let publisher = self.simple_publisher.clone();
            self.simple_local.write().subscribe(
                key,
                ex,
                Box::new(move |key, value, version, source| {
                    publisher.publish(key, (key, value, version, source));
                }),
            );
        }

        subscriber
    }

    pub fn hset(&self, key: KeyId, sub_key: SubKeyId, value: Vec<u8>, ex: Option<u64>) {
        self.hashmap_local.write().set(key, sub_key, value, ex);
    }

    pub async fn hget(&self, key: KeyId, timeout_ms: u64) -> Result<Option<Vec<(SubKeyId, ValueType, KeyVersion, KeySource)>>, HashmapKeyValueGetError> {
        let (tx, rx) = async_std::channel::bounded(1);
        let handler = move |res: Result<Option<Vec<(SubKeyId, ValueType, KeyVersion, KeySource)>>, HashmapKeyValueGetError>| {
            tx.try_send(res).print_error("Should send result");
        };
        self.hashmap_local.write().get(key, Box::new(handler), timeout_ms);
        rx.recv().await.unwrap_or(Err(HashmapKeyValueGetError::Timeout))
    }

    pub fn hdel(&self, key: KeyId, sub_key: SubKeyId) {
        self.hashmap_local.write().del(key, sub_key);
    }

    pub fn hsubscribe(&self, key: u64, ex: Option<u64>) -> HashmapKeyValueSubscriber {
        let local = self.hashmap_local.clone();
        let (subscriber, is_new) = self.hashmap_publisher.subscribe(
            key,
            Box::new(move || {
                local.write().unsubscribe(key);
            }),
        );
        if is_new {
            let publisher = self.hashmap_publisher.clone();
            self.hashmap_local.write().subscribe(
                key,
                ex,
                Box::new(move |key, sub_key, value, version, source| {
                    publisher.publish(key, (key, sub_key, value, version, source));
                }),
            );
        }

        subscriber
    }

    pub fn hsubscribe_raw(&self, key: u64, uuid: u64, ex: Option<u64>, tx: Sender<(KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource)>) {
        if self.hashmap_publisher.sub_raw(key, uuid, tx) {
            let publisher = self.hashmap_publisher.clone();
            self.hashmap_local.write().subscribe(
                key,
                ex,
                Box::new(move |key, sub_key, value, version, source| {
                    publisher.publish(key, (key, sub_key, value, version, source));
                }),
            );
        }
    }

    pub fn hunsubscribe_raw(&self, key: u64, uuid: u64) {
        if self.hashmap_publisher.unsub_raw(key, uuid) {
            self.hashmap_local.write().unsubscribe(key);
        }
    }
}

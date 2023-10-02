use std::sync::Arc;

use parking_lot::RwLock;
use utils::error_handle::ErrorUtils;

use crate::{KeyId, KeyVersion, ValueType};

use super::simple_local::{LocalStorage, SimpleKeyValueGetError};

mod pub_sub;

pub type KeyValueSubscriber = pub_sub::Subscriber<u64, (KeyId, Option<ValueType>, KeyVersion)>;

pub struct KeyValueSdk {
    local: Arc<RwLock<LocalStorage>>,
    publisher: Arc<pub_sub::PublisherManager<u64, (KeyId, Option<ValueType>, KeyVersion)>>,
}

impl KeyValueSdk {
    pub fn new(local: Arc<RwLock<LocalStorage>>) -> Self {
        Self {
            local,
            publisher: Arc::new(pub_sub::PublisherManager::new()),
        }
    }

    pub fn set(&mut self, key: u64, value: Vec<u8>, ex: Option<u64>) {
        self.local.write().set(key, value, ex);
    }

    pub async fn get(&self, key: u64, timeout_ms: u64) -> Result<Option<(ValueType, KeyVersion)>, SimpleKeyValueGetError> {
        let (tx, rx) = async_std::channel::bounded(1);
        let handler = move |res: Result<Option<(ValueType, KeyVersion)>, SimpleKeyValueGetError>| {
            tx.try_send(res).print_error("Should send result");
        };
        self.local.write().get(key, Box::new(handler), timeout_ms);
        rx.recv().await.unwrap_or(Err(SimpleKeyValueGetError::Timeout))
    }

    pub fn del(&mut self, key: u64) {
        self.local.write().del(key);
    }

    pub fn subscribe(&mut self, key: u64, ex: Option<u64>) -> KeyValueSubscriber {
        let local = self.local.clone();
        let (subscriber, is_new) = self.publisher.subscribe(
            key,
            Box::new(move || {
                local.write().unsubscribe(key);
            }),
        );
        if is_new {
            let publisher = self.publisher.clone();
            self.local.write().subscribe(
                key,
                ex,
                Box::new(move |key, value, version| {
                    publisher.publish(key, (key, value, version));
                }),
            );
        }

        subscriber
    }
}

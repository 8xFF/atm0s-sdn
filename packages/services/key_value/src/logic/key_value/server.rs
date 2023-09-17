use crate::{
    msg::{StorageAction, StorageActionRetryStrategy, StorageActionRouting},
    KeyId, ValueType,
};
use bluesea_router::RouterTable;
use key_value_storage::simple::SimpleKeyValue;
use std::sync::Arc;
use utils::{random::Random, Timer};

use super::{KeyValueClientEvent, KeyValueServerAction};

pub struct KeyValueServer {
    storage: SimpleKeyValue<KeyId, ValueType, u32>,
    random: Arc<dyn Random<u64>>,
    router: Arc<dyn RouterTable>,
}

impl KeyValueServer {
    pub fn new(router: Arc<dyn RouterTable>, random: Arc<dyn Random<u64>>, timer: Arc<dyn Timer>) -> Self {
        Self {
            storage: SimpleKeyValue::new(timer.clone()),
            random,
            router,
        }
    }

    pub fn on_remote(&mut self, action: KeyValueServerAction) {
        match action {
            KeyValueServerAction::Set(key, value, version, ex) => {
                self.storage.set(key, value, version, ex);
            }
            KeyValueServerAction::Del(key, value) => {
                self.storage.del(&key);
            }
            KeyValueServerAction::Sub(key, dest_node, ex) => {
                self.storage.subscribe(&key, dest_node, ex);
            }
            KeyValueServerAction::UnSub(key, dest_node) => {
                self.storage.unsubscribe(&key, &dest_node);
            }
        }
    }

    pub fn tick(&mut self) {
        self.storage.tick();
    }

    pub fn poll(&mut self) -> Option<StorageAction> {
        loop {
            let event = self.storage.poll()?;
            match event {
                key_value_storage::simple::OutputEvent::NotifySet(key, value, version, dest_node) => {
                    if self.router.path_to_key(key as u32).is_local() {
                        return Some(StorageAction::make::<KeyValueClientEvent>(
                            &self.random,
                            StorageActionRouting::Node(dest_node),
                            key,
                            KeyValueClientEvent::NotifySet(key, value, version).into(),
                            StorageActionRetryStrategy::Retry(10),
                        ));
                    }
                }
                key_value_storage::simple::OutputEvent::NotifyDel(key, _value, version, dest_node) => {
                    if self.router.path_to_key(key as u32).is_local() {
                        return Some(StorageAction::make::<KeyValueClientEvent>(
                            &self.random,
                            StorageActionRouting::Node(dest_node),
                            key,
                            KeyValueClientEvent::NotifyDel(key, version).into(),
                            StorageActionRetryStrategy::Retry(10),
                        ));
                    }
                }
            }
        }
    }
}

use crate::{
    msg::{StorageAction, StorageActionRetryStrategy, StorageActionRouting},
    KeyId,
};
use key_value_storage::simple::SimpleKeyValue;
use std::sync::Arc;
use utils::{random::Random, Timer};

use super::{KeyValueClientEvents, KeyValueServerActions};

pub struct KeyValueServer {
    storage: SimpleKeyValue<KeyId, Vec<u8>, u32>,
    random: Arc<dyn Random<u64>>,
}

impl KeyValueServer {
    pub fn new(random: Arc<dyn Random<u64>>, timer: Arc<dyn Timer>) -> Self {
        Self {
            storage: SimpleKeyValue::new(timer.clone()),
            random,
        }
    }

    pub fn on_remote(&mut self, action: KeyValueServerActions) {
        match action {
            KeyValueServerActions::Set(key, value, version, ex) => {
                self.storage.set(key, value, version, ex);
            }
            KeyValueServerActions::Del(key, value) => {
                self.storage.del(&key);
            }
            KeyValueServerActions::Sub(key, dest_node, ex) => {
                self.storage.subscribe(&key, dest_node, ex);
            }
            KeyValueServerActions::UnSub(key, dest_node) => {
                self.storage.unsubscribe(&key, &dest_node);
            }
        }
    }

    pub fn tick(&mut self) {
        self.storage.tick();
    }

    pub fn poll(&mut self) -> Option<StorageAction> {
        let event = self.storage.poll()?;
        match event {
            key_value_storage::simple::OutputEvent::NotifySet(key, value, version, dest_node) => {
                //TODO check if this node is the closest node to the key
                Some(StorageAction::make::<KeyValueClientEvents>(
                    &self.random,
                    StorageActionRouting::Node(dest_node),
                    key,
                    KeyValueClientEvents::NotifySet(key, value, version).into(),
                    StorageActionRetryStrategy::Retry(10),
                ))
            }
            key_value_storage::simple::OutputEvent::NotifyDel(key, _value, version, dest_node) => {
                //TODO check if this node is the closest node to the key
                Some(StorageAction::make::<KeyValueClientEvents>(
                    &self.random,
                    StorageActionRouting::Node(dest_node),
                    key,
                    KeyValueClientEvents::NotifyDel(key, version).into(),
                    StorageActionRetryStrategy::Retry(10),
                ))
            }
        }
    }
}

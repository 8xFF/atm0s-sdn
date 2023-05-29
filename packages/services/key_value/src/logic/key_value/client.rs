//! This module contains the implementation of a key-value storage client in Rust.
//!
//! The client provides methods for setting, deleting, and subscribing to key-value pairs, and uses a timer and event queue to manage synchronization with other nodes in the system.
//!
//! The `set` method inserts a new key-value pair into the local storage and generates three `Set` actions with three synchronization keys.
//!
//! The `del` method deletes a key-value pair from the local storage and generates three `Del` actions with three synchronization keys.
//!
//! The file also defines a `KeyValueClient` struct that encapsulates the client's state and provides methods for interacting with the storage and event queue.
//!
//! # Examples
//!
//! ```
//! use bluesea_identity::{NodeId, NodeIdType};
//! use std::sync::Arc;
//! use key_value::logic::key_value::client::KeyValueClient;
//! use utils::MockTimer;
//! use utils::random::MockRandom;
//!
//! let local_node_id = NodeId::random();
//! let random = Arc::new(MockRandom::default());
//! let timer = Arc::new(MockTimer::default());
//! let mut client = KeyValueClient::new(local_node_id, random, timer);
//!
//! let key = 123;
//! let value = vec![1, 2, 3];
//! let ex = Some(60);
//!
//! // Call the set method to add a key-value pair to the storage
//! client.set(key, value.clone(), ex);
//!
//! // Call the poll method to get the next event from the queue
//! let event = client.poll();
//! ```
use bluesea_identity::NodeId;
use std::sync::Arc;
use utils::{hashmap::HashMap, random::Random, vec_dequeue::VecDeque, Timer};

use crate::{
    msg::{StorageAction, StorageActionRetryStrategy, StorageActionRouting},
    KeyId, ValueType,
};

use super::KeyValueServerActions;

struct Slot {
    value: ValueType,
    version: u64,
    ex: Option<u64>,
    fired_at: u64,
}

struct SubSlot {
    ex: Option<u64>,
    fired_at: u64,
}

pub struct KeyValueClient {
    local_node_id: NodeId,
    storage: HashMap<KeyId, Slot>,
    subscribes: HashMap<KeyId, SubSlot>,
    events: VecDeque<StorageAction>,
    timer: Arc<dyn Timer>,
    rand: Arc<dyn Random<u64>>,
}

impl KeyValueClient {
    pub fn new(local_node_id: NodeId, rand: Arc<dyn Random<u64>>, timer: Arc<dyn Timer>) -> Self {
        Self {
            local_node_id,
            timer,
            rand,
            storage: Default::default(),
            events: Default::default(),
            subscribes: Default::default(),
        }
    }

    /// Insert with version is current time in miliseconds, after inserted, create 3 Set actions with 3 sync keys
    pub fn set(&mut self, key: u64, value: Vec<u8>, ex: Option<u64>) {
        let version = self.timer.now_ms();
        let slot = Slot {
            value: value.clone(),
            version: self.timer.now_ms(),
            ex,
            fired_at: self.timer.now_ms(),
        };
        self.storage.insert(key, slot);
        //TODO generate replicate keys
        let routing_keys = vec![key];
        for routing_key in routing_keys {
            let event: StorageAction = StorageAction::make::<KeyValueServerActions>(
                &self.rand,
                StorageActionRouting::ClosestNode(routing_key),
                key,
                KeyValueServerActions::Set(key, value.clone(), version, ex),
                StorageActionRetryStrategy::Retry(10),
            );
            self.events.push_back(event);
        }
    }

    /// Delete value in local mode and create 3 Del actions with 3 sync keys
    pub fn del(&mut self, key: &u64) -> Option<(Vec<u8>, u64)> {
        if let Some(slot) = self.storage.remove(key) {
            //TODO generate replicate keys
            let routing_keys = vec![*key];
            for routing_key in routing_keys {
                let event = StorageAction::make::<KeyValueServerActions>(
                    &self.rand,
                    StorageActionRouting::ClosestNode(routing_key),
                    *key,
                    KeyValueServerActions::Del(*key, slot.version),
                    StorageActionRetryStrategy::Retry(10),
                );
                self.events.push_back(event);
            }
            Some((slot.value, slot.version))
        } else {
            None
        }
    }

    /// Subscribe to a key
    pub fn subscribe(&mut self, key: &u64, ex: Option<u64>) {
        //TODO generate replicate keys
        let routing_keys = vec![*key];
        for routing_key in routing_keys {
            let event = StorageAction::make::<KeyValueServerActions>(
                &self.rand,
                StorageActionRouting::ClosestNode(routing_key),
                *key,
                KeyValueServerActions::Sub(*key, self.local_node_id, ex),
                StorageActionRetryStrategy::Retry(10),
            );
            self.events.push_back(event);
        }
        self.subscribes.insert(
            *key,
            SubSlot {
                ex,
                fired_at: self.timer.now_ms(),
            },
        );
    }

    /// Unsubscribe to a key
    pub fn unsubscribe(&mut self, key: &u64) {
        //TODO generate replicate keys
        let routing_keys = vec![*key];
        for routing_key in routing_keys {
            let event = StorageAction::make::<KeyValueServerActions>(
                &self.rand,
                StorageActionRouting::ClosestNode(routing_key),
                *key,
                KeyValueServerActions::UnSub(*key, self.local_node_id),
                StorageActionRetryStrategy::Retry(10),
            );
            self.events.push_back(event);
        }
        self.subscribes.remove(key);
    }

    pub fn tick(&mut self) {
        // In each 10 seconds resend Set action for all stored keys
        let now_ms = self.timer.now_ms();
        for (key, slot) in self.storage.iter_mut() {
            if now_ms - slot.fired_at > 10_000 {
                let event = StorageAction::make::<KeyValueServerActions>(
                    &self.rand,
                    StorageActionRouting::ClosestNode(*key),
                    *key,
                    KeyValueServerActions::Set(*key, slot.value.clone(), slot.version, slot.ex),
                    StorageActionRetryStrategy::Retry(10),
                );
                self.events.push_back(event);
                slot.fired_at = now_ms;
            }
        }

        // In each 10 seconds resend Sub action for all subscribed keys
        for (key, slot) in self.subscribes.iter_mut() {
            if now_ms - slot.fired_at > 10_000 {
                let event = StorageAction::make::<KeyValueServerActions>(
                    &self.rand,
                    StorageActionRouting::ClosestNode(*key),
                    *key,
                    KeyValueServerActions::Sub(*key, self.local_node_id, slot.ex),
                    StorageActionRetryStrategy::Retry(10),
                );
                self.events.push_back(event);
                slot.fired_at = now_ms;
            }
        }
    }

    /// Get value from local storage
    pub fn poll(&mut self) -> Option<StorageAction> {
        self.events.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use utils::{random::MockRandom, MockTimer};

    use super::*;

    #[test]
    fn test_set() {
        let random: Arc<dyn Random<u64>> = Arc::new(MockRandom::default());
        let timer = Arc::new(MockTimer::default());
        timer.fake(0);
        let mut client = KeyValueClient::new(1, random.clone(), timer.clone());
        let key = 123;
        let value = vec![1, 2, 3];
        let ex = Some(60);

        // Call the set method to add a key-value pair to the storage
        client.set(key, value.clone(), ex);
        assert_eq!(client.storage.len(), 1);

        // Call the poll method to get the next event from the queue
        let event = client.poll();

        // Check that the event is a Set action for the stored key
        assert_eq!(
            event,
            Some(StorageAction::make::<KeyValueServerActions>(
                &random,
                StorageActionRouting::ClosestNode(key),
                key,
                KeyValueServerActions::Set(key, value, 0, ex),
                StorageActionRetryStrategy::Retry(10),
            ))
        );
    }

    #[test]
    fn test_del() {
        let random: Arc<dyn Random<u64>> = Arc::new(MockRandom::default());
        let timer = Arc::new(MockTimer::default());
        timer.fake(0);
        let mut client = KeyValueClient::new(1, random.clone(), timer.clone());
        let key = 123;
        let value = vec![1, 2, 3];
        let ex = Some(60);

        // Add a key-value pair to the storage
        client.set(key, value, ex);
        assert_eq!(client.storage.len(), 1);

        client.poll().expect("poll should return an event");

        // Call the del method to remove the key-value pair from the storage
        client.del(&key);
        assert_eq!(client.storage.len(), 0);

        // Call the poll method to get the next event from the queue
        let event = client.poll();

        // Check that the event is a Del action for the removed key
        assert_eq!(
            event,
            Some(StorageAction::make::<KeyValueServerActions>(
                &random,
                StorageActionRouting::ClosestNode(key),
                key,
                KeyValueServerActions::Del(key, 0),
                StorageActionRetryStrategy::Retry(10),
            ))
        );
    }

    #[test]
    fn test_sub() {
        let random: Arc<dyn Random<u64>> = Arc::new(MockRandom::default());
        let timer = Arc::new(MockTimer::default());
        timer.fake(0);
        let mut client = KeyValueClient::new(1, random.clone(), timer.clone());
        let key = 123;
        let ex = Some(60);

        // Call the sub method to subscribe to a key
        client.subscribe(&key, ex);
        assert_eq!(client.subscribes.len(), 1);

        // Call the poll method to get the next event from the queue
        let event = client.poll();

        // Check that the event is a Sub action for the subscribed key
        assert_eq!(
            event,
            Some(StorageAction::make::<KeyValueServerActions>(
                &random,
                StorageActionRouting::ClosestNode(key),
                key,
                KeyValueServerActions::Sub(key, client.local_node_id, ex),
                StorageActionRetryStrategy::Retry(10),
            ))
        );
    }

    #[test]
    fn test_unsub() {
        let random: Arc<dyn Random<u64>> = Arc::new(MockRandom::default());
        let timer = Arc::new(MockTimer::default());
        timer.fake(0);
        let mut client = KeyValueClient::new(1, random.clone(), timer.clone());
        let key = 123;
        let ex = Some(60);

        // Call the sub method to subscribe to a key
        client.subscribe(&key, ex);
        assert_eq!(client.subscribes.len(), 1);

        client.poll().expect("poll should return an event");

        // Call the unsub method to unsubscribe from the key
        client.unsubscribe(&key);
        assert_eq!(client.subscribes.len(), 0);

        // Call the poll method to get the next event from the queue
        let event = client.poll();

        // Check that the event is an Unsub action for the unsubscribed key
        assert_eq!(
            event,
            Some(StorageAction::make::<KeyValueServerActions>(
                &random,
                StorageActionRouting::ClosestNode(key),
                key,
                KeyValueServerActions::UnSub(key, client.local_node_id),
                StorageActionRetryStrategy::Retry(10),
            ))
        );
    }

    #[test]
    fn test_tick() {
        let random: Arc<dyn Random<u64>> = Arc::new(MockRandom::default());
        let timer = Arc::new(MockTimer::default());
        timer.fake(0);
        let mut client = KeyValueClient::new(1, random.clone(), timer.clone());
        let key = 123;
        let value = vec![1, 2, 3];
        let ex = Some(60);

        // Add a key-value pair to the storage
        client.set(key, value.clone(), ex);

        // Fake the timer to 10 seconds
        timer.fake(10_000);

        // Call the tick method to resend the Set action for the stored key
        client.tick();

        // Check that the Set action was added to the events queue
        assert_eq!(
            client.events.pop_front(),
            Some(StorageAction::make::<KeyValueServerActions>(
                &random,
                StorageActionRouting::ClosestNode(key),
                key,
                KeyValueServerActions::Set(
                    key,
                    value,
                    client.storage.get(&key).unwrap().version,
                    ex
                ),
                StorageActionRetryStrategy::Retry(10),
            ))
        );
    }
}

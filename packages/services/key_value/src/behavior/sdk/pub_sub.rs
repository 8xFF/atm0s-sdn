use std::{
    collections::HashMap,
    hash::Hash,
    sync::{atomic::AtomicU64, Arc},
};

use async_std::channel::{Receiver, Sender};
use p_8xff_sdn_utils::error_handle::ErrorUtils;
use parking_lot::RwLock;

struct SubscribeContainer<T> {
    subscribers: HashMap<u64, Sender<T>>,
    clear_handler: Box<dyn FnOnce() + Send + Sync>,
}

pub struct PublisherManager<K, T> {
    uuid: AtomicU64,
    subscribers: Arc<RwLock<HashMap<K, SubscribeContainer<T>>>>,
}

impl<K: Hash + Eq + Copy, T: Clone> PublisherManager<K, T> {
    pub fn new() -> Self {
        Self {
            uuid: AtomicU64::new(0),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn sub_raw(&self, key: K, uuid: u64, tx: Sender<T>) -> bool {
        let mut subscribers = self.subscribers.write();
        match subscribers.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().subscribers.insert(uuid, tx);
                false
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(SubscribeContainer {
                    subscribers: HashMap::from([(uuid, tx)]),
                    clear_handler: Box::new(|| {}),
                });
                true
            }
        }
    }

    pub fn unsub_raw(&self, key: K, uuid: u64) -> bool {
        let mut subscribers = self.subscribers.write();
        let should_remove = {
            if let Some(container) = subscribers.get_mut(&key) {
                container.subscribers.remove(&uuid);
                container.subscribers.is_empty()
            } else {
                false
            }
        };

        if should_remove {
            if let Some(container) = subscribers.remove(&key) {
                (container.clear_handler)();
            }
        }

        should_remove
    }

    /// subscribe and return Subscriber and is_new
    /// is_new is true if this is the first subscriber
    /// is_new is false if this is not the first subscriber
    /// If Subscriber is drop, it automatically unsubscribe
    pub fn subscribe(&self, key: K, clear_handler: Box<dyn FnOnce() + Send + Sync>) -> (Subscriber<K, T>, bool) {
        let mut subscribers = self.subscribers.write();
        let uuid = self.uuid.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (tx, rx) = async_std::channel::unbounded();
        let is_new = match subscribers.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().subscribers.insert(uuid, tx);
                false
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(SubscribeContainer {
                    subscribers: HashMap::from([(uuid, tx)]),
                    clear_handler,
                });
                true
            }
        };

        (
            Subscriber {
                uuid,
                key,
                subscribers: self.subscribers.clone(),
                rx,
            },
            is_new,
        )
    }

    pub fn publish(&self, key: K, data: T) {
        let subscribers = self.subscribers.read();
        if let Some(container) = subscribers.get(&key) {
            for (_, tx) in container.subscribers.iter() {
                tx.send_blocking(data.clone()).print_error("Should send event");
            }
        }
    }
}

pub struct Subscriber<K: Hash + Eq + Copy, T> {
    uuid: u64,
    key: K,
    subscribers: Arc<RwLock<HashMap<K, SubscribeContainer<T>>>>,
    rx: Receiver<T>,
}

impl<K: Hash + Eq + Copy, T> Subscriber<K, T> {
    pub async fn recv(&mut self) -> Option<T> {
        self.rx.recv().await.ok()
    }
}

impl<K: Hash + Eq + Copy, T> Drop for Subscriber<K, T> {
    fn drop(&mut self) {
        let mut subscribers = self.subscribers.write();
        let should_remove = {
            let container = subscribers.get_mut(&self.key).expect("Should have subscribers");
            container.subscribers.remove(&self.uuid);
            container.subscribers.is_empty()
        };

        if should_remove {
            if let Some(container) = subscribers.remove(&self.key) {
                (container.clear_handler)();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicU8, Arc};

    #[test]
    fn test_simple_pubsub() {
        let pub_manager = super::PublisherManager::<u64, u64>::new();

        {
            let destroy_count = Arc::new(AtomicU8::new(0));
            let destroy_count_c = destroy_count.clone();
            let (mut sub1, is_new) = pub_manager.subscribe(
                1,
                Box::new(move || {
                    destroy_count_c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
            );
            assert!(is_new);
            let (mut sub2, is_new) = pub_manager.subscribe(1, Box::new(|| {}));
            assert!(!is_new);
            let destroy_count_c = destroy_count.clone();
            let (mut sub3, is_new) = pub_manager.subscribe(
                2,
                Box::new(move || {
                    destroy_count_c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }),
            );
            assert!(is_new);

            pub_manager.publish(1, 1);
            pub_manager.publish(2, 2);

            // sub1 should receive 1 with timeout 1s
            assert_eq!(async_std::task::block_on(sub1.recv()), Some(1));
            // sub2 should receive 1 with timeout 1s
            assert_eq!(async_std::task::block_on(sub2.recv()), Some(1));
            // sub3 should receive 2 with timeout 1s
            assert_eq!(async_std::task::block_on(sub3.recv()), Some(2));

            // drop sub1
            drop(sub1);
            // drop sub2
            drop(sub2);
            // drop sub3
            drop(sub3);

            // destroy_count should be 2
            assert_eq!(destroy_count.load(std::sync::atomic::Ordering::SeqCst), 2);
        }

        //after drop subscribers should be empty
        assert!(pub_manager.subscribers.read().is_empty());
    }

    #[test]
    fn test_simple_pubsub_memory() {
        let pub_manager = super::PublisherManager::<u64, u64>::new();

        let info = allocation_counter::measure(|| {
            let (sub1, is_new) = pub_manager.subscribe(1, Box::new(|| {}));
            assert!(is_new);
            let (sub2, is_new) = pub_manager.subscribe(1, Box::new(|| {}));
            assert!(!is_new);
            let (sub3, is_new) = pub_manager.subscribe(2, Box::new(|| {}));
            assert!(is_new);

            // drop sub1
            drop(sub1);
            // drop sub2
            drop(sub2);
            // drop sub3
            drop(sub3);

            // shrink to fit for check memory leak
            pub_manager.subscribers.write().shrink_to_fit();

            //after drop subscribers should be empty
            assert!(pub_manager.subscribers.read().is_empty());
        });
        assert_eq!(info.count_current, 0);
    }
}

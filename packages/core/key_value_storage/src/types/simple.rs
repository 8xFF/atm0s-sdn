use std::hash::Hash;
use std::sync::Arc;
use utils::hashmap::HashMap;
use utils::vec_dequeue::VecDeque;
use utils::Timer;

#[derive(Eq, PartialEq, Debug)]
pub enum OutputEvent<K, V, H> {
    NotifySet(K, V, u64, H),
    NotifyDel(K, V, u64, H),
}

struct HandlerSlot {
    #[allow(dead_code)]
    added_at: u64,
    expire_at: Option<u64>,
}

struct ValueSlot<V, H> {
    value: Option<(V, u64)>,
    expire_at: Option<u64>,
    listeners: HashMap<H, HandlerSlot>,
}

pub struct SimpleKeyValue<K, V, H> {
    keys: HashMap<K, ValueSlot<V, H>>,
    events: VecDeque<OutputEvent<K, V, H>>,
    timer: Arc<dyn Timer>,
}

impl<K, V, H> SimpleKeyValue<K, V, H>
where
    K: PartialEq + Eq + Hash + Clone,
    V: Clone,
    H: PartialEq + Eq + Hash + Clone,
{
    pub fn new(timer: Arc<dyn Timer>) -> Self {
        Self {
            keys: Default::default(),
            events: Default::default(),
            timer,
        }
    }

    /// This function is call in each tick miliseconds, this function will check all expire time
    /// and clear all expired data, and fire all expire events
    /// It also clear expired handlers
    pub fn tick(&mut self) {
        let now = self.timer.now_ms();
        // Set value of key to None if it is expired and fire del event
        let mut expired_keys = vec![];
        for (key, slot) in self.keys.iter_mut() {
            if let Some(expire_at) = slot.expire_at {
                if expire_at <= now {
                    expired_keys.push(key.clone());
                }
            }
        }
        for key in expired_keys {
            self.del(&key);
        }

        // Clear expired handlers
        for (_, slot) in self.keys.iter_mut() {
            let mut expired_handlers = vec![];
            for (handler, handler_slot) in slot.listeners.iter() {
                if let Some(expire_at) = handler_slot.expire_at {
                    if expire_at <= now {
                        expired_handlers.push(handler.clone());
                    }
                }
            }
            for handler in expired_handlers {
                slot.listeners.remove(&handler);
            }
        }

        // Clear key if it value is None and no handlers
        let mut empty_keys = vec![];
        for (key, slot) in self.keys.iter() {
            if slot.value.is_none() && slot.listeners.is_empty() {
                empty_keys.push(key.clone());
            }
        }
        for key in empty_keys {
            self.keys.remove(&key);
        }
    }

    /// EX seconds -- Set the specified expire time, in seconds.
    /// version -- Version of value, it should be increased each time new data is set
    pub fn set(&mut self, key: K, value: V, version: u64, ex: Option<u64>) -> bool {
        match self.keys.get_mut(&key) {
            Some(slot) => {
                let should_add = match &slot.value {
                    Some((_, old_version)) => *old_version < version,
                    None => true,
                };
                if should_add {
                    slot.value = Some((value, version));
                    slot.expire_at = ex.map(|ex| self.timer.now_ms() + ex);
                    Self::fire_set_events(&key, slot, &mut self.events);
                    true
                } else {
                    false
                }
            }
            None => {
                let slot = ValueSlot {
                    value: Some((value, version)),
                    expire_at: ex.map(|ex| self.timer.now_ms() + ex),
                    listeners: Default::default(),
                };
                Self::fire_set_events(&key, &slot, &mut self.events);
                self.keys.insert(key, slot);
                true
            }
        }
    }

    pub fn get(&self, key: &K) -> Option<(&V, u64)> {
        let slot = self.keys.get(key)?;
        if let Some((value, version)) = &(slot.value) {
            Some((value, *version))
        } else {
            None
        }
    }

    pub fn del(&mut self, key: &K) -> Option<(V, u64)> {
        if let Some(slot) = self.keys.get_mut(key) {
            if slot.value.is_some() {
                Self::fire_del_events(key, slot, &mut self.events);
                let (value, version) = slot.value.take().expect("cannot happend");
                slot.expire_at = None;
                if slot.listeners.is_empty() {
                    self.keys.remove(key);
                }
                Some((value, version))
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn subscribe(&mut self, key: &K, handler_uuid: H, ex: Option<u64>) {
        match self.keys.get_mut(&key) {
            Some(slot) => {
                slot.listeners.insert(
                    handler_uuid,
                    HandlerSlot {
                        added_at: self.timer.now_ms(),
                        expire_at: ex.map(|ex| self.timer.now_ms() + ex),
                    },
                );
            }
            None => {
                self.keys.insert(
                    key.clone(),
                    ValueSlot {
                        value: None,
                        expire_at: None,
                        listeners: HashMap::from([(
                            handler_uuid,
                            HandlerSlot {
                                added_at: self.timer.now_ms(),
                                expire_at: ex.map(|ex| self.timer.now_ms() + ex),
                            },
                        )]),
                    },
                );
            }
        }
    }

    pub fn unsubscribe(&mut self, key: &K, handler_uuid: &H) -> bool {
        match self.keys.get_mut(&key) {
            None => false,
            Some(slot) => {
                if slot.listeners.remove(handler_uuid).is_some() {
                    if slot.value.is_none() && slot.listeners.is_empty() {
                        self.keys.remove(key);
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn poll(&mut self) -> Option<OutputEvent<K, V, H>> {
        self.events.pop_front()
    }

    //Private functions
    ///Fire set events of slot, this should be called after new data is set
    fn fire_set_events(key: &K, slot: &ValueSlot<V, H>, events: &mut VecDeque<OutputEvent<K, V, H>>) -> bool {
        if let Some((value, version)) = &slot.value {
            for (handler, _handler_slot) in slot.listeners.iter() {
                events.push_back(OutputEvent::NotifySet(key.clone(), value.clone(), *version, handler.clone()));
            }
            true
        } else {
            false
        }
    }

    ///Fire del events of slot, this should be called before delete data
    fn fire_del_events(key: &K, slot: &ValueSlot<V, H>, events: &mut VecDeque<OutputEvent<K, V, H>>) -> bool {
        if let Some((value, version)) = &slot.value {
            for (handler, _handler_slot) in slot.listeners.iter() {
                events.push_back(OutputEvent::NotifyDel(key.clone(), value.clone(), *version, handler.clone()));
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(test)]
    #[global_allocator]
    static ALLOC: dhat::Alloc = dhat::Alloc;

    /// Test cases:
    /// 1. Set value
    /// 2. Get value
    /// 3. Delete value
    /// 4. Subscribe
    /// 5. Unsubscribe
    /// 6. Expire
    /// 7. Expire handler
    /// 8. Expire handler and value
    use super::*;
    use utils::MockTimer;

    /// This test case using MockTimer to simulate time and set value
    #[test]
    fn set_value() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        assert!(store.set(key, value, version, None));
        assert_eq!(store.get(&key), Some((&value, version)));
    }

    /// Must return None after delete
    #[test]
    fn delete_value() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        assert!(store.set(key, value, version, None));
        assert_eq!(store.del(&key), Some((value, version)));
        assert_eq!(store.get(&key), None);
    }

    /// Must auto clear key after expire
    #[test]
    fn expire_value() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        assert!(store.set(key, value, version, Some(100)));
        assert_eq!(store.get(&key), Some((&value, version)));
        timer.fake(100);
        store.tick();
        assert_eq!(store.get(&key), None);
    }

    /// Must add event after subscribe and set
    #[test]
    fn subscribe() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        store.subscribe(&key, handler, None);
        assert!(store.set(key, value, version, None));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must add multi event after subscribe multi handlers and set
    #[test]
    fn subscribe_multi_handlers() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        let handler1 = 1;
        let handler2 = 2;
        store.subscribe(&key, handler1, None);
        store.subscribe(&key, handler2, None);
        assert!(store.set(key, value, version, None));
        let event1 = store.poll().expect("Should return NotifySet");
        let event2 = store.poll().expect("Should return NotifySet");

        let expected1 = OutputEvent::NotifySet(key, value, version, handler1);
        let expected2 = OutputEvent::NotifySet(key, value, version, handler2);

        assert!(event1 == expected1 || event1 == expected2);
        assert!(event2 == expected1 || event2 == expected2);
        assert_ne!(event1, event2);

        assert_eq!(store.poll(), None);
    }

    /// Must not add event after unsubscribe and set
    #[test]
    fn unsubscribe() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        store.subscribe(&key, handler, None);
        assert!(store.unsubscribe(&key, &handler));
        assert!(store.set(key, value, version, None));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and del
    #[test]
    fn subscribe_del() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        store.subscribe(&key, handler, None);
        assert!(store.set(key, value, version, None));
        assert_eq!(store.del(&key), Some((value, version)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, handler)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key, value, version, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must not add event after unsubscribe and del
    #[test]
    fn unsubscribe_del() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        store.subscribe(&key, handler, None);
        assert!(store.unsubscribe(&key, &handler));
        assert!(store.set(key, value, version, None));
        assert_eq!(store.del(&key), Some((value, version)));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and data expire
    #[test]
    fn subscribe_expire() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        store.subscribe(&key, handler, None);
        assert!(store.set(key, value, version, Some(100)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, handler)));
        assert_eq!(store.poll(), None);
        timer.fake(100);
        store.tick();
        assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key, value, version, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must clear handler after expire
    #[test]
    fn expire_handler() {
        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        store.subscribe(&key, handler, Some(100));
        assert!(store.set(key, value, version, None));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, handler)));
        timer.fake(100);
        store.tick();
        assert_eq!(store.del(&key), Some((value, version)));
        assert_eq!(store.poll(), None);
    }

    /// Should clear memory after delete
    #[test]
    fn delete_memory() {
        let _profiler = dhat::Profiler::builder().testing().build();

        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());

        let stats = dhat::HeapStats::get();

        let key1 = 1;
        let value1 = 2;
        let version1 = 1;
        assert!(store.set(key1, value1, version1, None));
        assert_eq!(store.del(&key1), Some((value1, version1)));
        assert_eq!(store.keys.len(), 0);
        assert_eq!(store.events.len(), 0);

        let new_stats = dhat::HeapStats::get();
        assert_eq!(new_stats.curr_blocks, stats.curr_blocks);
        assert_eq!(new_stats.curr_bytes, stats.curr_bytes);
    }

    /// Should clear memory after unsubscribe
    #[test]
    fn unsubscribe_memory() {
        let _profiler = dhat::Profiler::builder().testing().build();

        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());

        let stats = dhat::HeapStats::get();

        let key1 = 1;
        let handler1 = 1;
        store.subscribe(&key1, handler1, None);
        assert!(store.unsubscribe(&key1, &handler1));

        let new_stats = dhat::HeapStats::get();
        assert_eq!(new_stats.curr_blocks, stats.curr_blocks);
        assert_eq!(new_stats.curr_bytes, stats.curr_bytes);
    }

    /// Should clear memory after expire data
    #[test]
    fn expire_memory() {
        let _profiler = dhat::Profiler::builder().testing().build();

        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());

        let stats = dhat::HeapStats::get();

        let key1 = 1;
        let value1 = 2;
        let version1 = 1;
        assert!(store.set(key1, value1, version1, Some(100)));
        timer.fake(100);
        store.tick();

        let new_stats = dhat::HeapStats::get();
        assert_eq!(new_stats.curr_blocks, stats.curr_blocks);
        assert_eq!(new_stats.curr_bytes, stats.curr_bytes);
    }

    /// Should clear memory after expire handler
    #[test]
    fn expire_handler_memory() {
        let _profiler = dhat::Profiler::builder().testing().build();

        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());

        let stats = dhat::HeapStats::get();

        let key1 = 1;
        let handler1 = 1;
        store.subscribe(&key1, handler1, Some(100));
        timer.fake(100);
        store.tick();

        let new_stats = dhat::HeapStats::get();
        assert_eq!(new_stats.curr_blocks, stats.curr_blocks);
        assert_eq!(new_stats.curr_bytes, stats.curr_bytes);
    }

    /// Should clear memory after pop all events
    #[test]
    fn pop_all_memory() {
        let _profiler = dhat::Profiler::builder().testing().build();

        let timer = Arc::new(MockTimer::default());
        let mut store = SimpleKeyValue::<u32, u32, u32>::new(timer.clone());

        let key1 = 1;
        let value1 = 2;
        let version1 = 1;
        let handler1 = 1;
        store.subscribe(&key1, handler1, None);

        let stats = dhat::HeapStats::get();

        assert!(store.set(key1, value1, version1, None));
        assert_eq!(store.del(&key1), Some((value1, version1)));

        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key1, value1, version1, handler1)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key1, value1, version1, handler1)));
        assert_eq!(store.poll(), None);

        let new_stats = dhat::HeapStats::get();
        assert_eq!(new_stats.curr_blocks, stats.curr_blocks);
        assert_eq!(new_stats.curr_bytes, stats.curr_bytes);
    }
}

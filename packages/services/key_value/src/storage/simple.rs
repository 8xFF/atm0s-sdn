use atm0s_sdn_utils::hashmap::HashMap;
use atm0s_sdn_utils::vec_dequeue::VecDeque;
use std::collections::hash_map;
use std::hash::Hash;

#[derive(Eq, PartialEq, Debug)]
pub enum OutputEvent<Key, Value, Source, Handler> {
    NotifySet(Key, Value, u64, Source, Handler),
    NotifyDel(Key, Value, u64, Source, Handler),
}

struct HandlerSlot {
    #[allow(dead_code)]
    added_at: u64,
    expire_at: Option<u64>,
}

struct ValueSlot<Value, Source, Handler> {
    value: Option<(Value, u64, Source)>,
    expire_at: Option<u64>,
    listeners: HashMap<Handler, HandlerSlot>,
}

pub struct SimpleKeyValue<Key, Value, Source, Handlder> {
    keys: HashMap<Key, ValueSlot<Value, Source, Handlder>>,
    events: VecDeque<OutputEvent<Key, Value, Source, Handlder>>,
}

impl<Key, Value, Source, Handler> SimpleKeyValue<Key, Value, Source, Handler>
where
    Key: PartialEq + Eq + Hash + Clone,
    Value: Clone,
    Source: Clone,
    Handler: PartialEq + Eq + Hash + Clone,
{
    pub fn new() -> Self {
        Self {
            keys: Default::default(),
            events: Default::default(),
        }
    }

    #[allow(unused)]
    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    /// This function is call in each tick miliseconds, this function will check all expire time
    /// and clear all expired data, and fire all expire events
    /// It also clear expired handlers
    pub fn tick(&mut self, now: u64) {
        // Set value of key to None if it is expired and fire del event
        let mut expired_keys = vec![];
        for (key, slot) in self.keys.iter_mut() {
            if let Some(expire_at) = slot.expire_at {
                if expire_at <= now {
                    if let Some((_, version, _)) = &slot.value {
                        expired_keys.push((key.clone(), *version));
                    }
                }
            }
        }
        for (key, version) in expired_keys {
            self.del(&key, version);
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
    pub fn set(&mut self, now_ms: u64, key: Key, value: Value, version: u64, source: Source, ex: Option<u64>) -> bool {
        match self.keys.get_mut(&key) {
            Some(slot) => {
                let should_add = match &slot.value {
                    Some((_, old_version, _)) => *old_version < version,
                    None => true,
                };
                if should_add {
                    slot.value = Some((value, version, source));
                    slot.expire_at = ex.map(|ex| now_ms + ex);
                    Self::fire_set_events(&key, slot, &mut self.events);
                    true
                } else {
                    slot.expire_at = ex.map(|ex| now_ms + ex);
                    false
                }
            }
            None => {
                let slot = ValueSlot {
                    value: Some((value, version, source)),
                    expire_at: ex.map(|ex| now_ms + ex),
                    listeners: Default::default(),
                };
                Self::fire_set_events(&key, &slot, &mut self.events);
                self.keys.insert(key, slot);
                true
            }
        }
    }

    pub fn get(&self, key: &Key) -> Option<(&Value, u64, Source)> {
        let slot = self.keys.get(key)?;
        if let Some((value, version, source)) = &(slot.value) {
            Some((value, *version, source.clone()))
        } else {
            None
        }
    }

    pub fn del(&mut self, key: &Key, request_version: u64) -> Option<(Value, u64, Source)> {
        if let Some(slot) = self.keys.get_mut(key) {
            if slot.value.is_none() {
                return None;
            }

            if let Some((_, version, _)) = &slot.value {
                if *version > request_version {
                    return None;
                }
            }

            Self::fire_del_events(key, slot, &mut self.events);
            let (value, version, source) = slot.value.take().expect("cannot happend");
            slot.expire_at = None;
            if slot.listeners.is_empty() {
                self.keys.remove(key);
            }
            Some((value, version, source))
        } else {
            None
        }
    }

    /// return true if subscribe success, false if already subscribed
    pub fn subscribe(&mut self, now_ms: u64, key: &Key, handler_uuid: Handler, ex: Option<u64>) -> bool {
        match self.keys.get_mut(key) {
            Some(slot) => match slot.listeners.entry(handler_uuid.clone()) {
                hash_map::Entry::Occupied(mut sub_slot) => {
                    sub_slot.get_mut().expire_at = ex.map(|ex| now_ms + ex);
                    false
                }
                hash_map::Entry::Vacant(sub_slot) => {
                    sub_slot.insert(HandlerSlot {
                        added_at: now_ms,
                        expire_at: ex.map(|ex| now_ms + ex),
                    });
                    if let Some((value, version, source)) = &slot.value {
                        self.events.push_back(OutputEvent::NotifySet(key.clone(), value.clone(), *version, source.clone(), handler_uuid));
                    }
                    true
                }
            },
            None => {
                self.keys.insert(
                    key.clone(),
                    ValueSlot {
                        value: None,
                        expire_at: None,
                        listeners: HashMap::from([(
                            handler_uuid,
                            HandlerSlot {
                                added_at: now_ms,
                                expire_at: ex.map(|ex| now_ms + ex),
                            },
                        )]),
                    },
                );
                true
            }
        }
    }

    pub fn unsubscribe(&mut self, key: &Key, handler_uuid: &Handler) -> bool {
        match self.keys.get_mut(key) {
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

    pub fn poll(&mut self) -> Option<OutputEvent<Key, Value, Source, Handler>> {
        self.events.pop_front()
    }

    //Private functions
    ///Fire set events of slot, this should be called after new data is set
    fn fire_set_events(key: &Key, slot: &ValueSlot<Value, Source, Handler>, events: &mut VecDeque<OutputEvent<Key, Value, Source, Handler>>) -> bool {
        if let Some((value, version, source)) = &slot.value {
            for (handler, _handler_slot) in slot.listeners.iter() {
                events.push_back(OutputEvent::NotifySet(key.clone(), value.clone(), *version, source.clone(), handler.clone()));
            }
            true
        } else {
            false
        }
    }

    ///Fire del events of slot, this should be called before delete data
    fn fire_del_events(key: &Key, slot: &ValueSlot<Value, Source, Handler>, events: &mut VecDeque<OutputEvent<Key, Value, Source, Handler>>) -> bool {
        if let Some((value, version, source)) = &slot.value {
            for (handler, _handler_slot) in slot.listeners.iter() {
                events.push_back(OutputEvent::NotifyDel(key.clone(), value.clone(), *version, source.clone(), handler.clone()));
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
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

    /// This test case using MockTimer to simulate time and set value
    #[test]
    fn set_value() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let source = 1000;
        assert!(store.set(0, key, value, source, version, None));
        assert_eq!(store.get(&key), Some((&value, source, version)));
    }

    /// Must return None after delete
    #[test]
    fn delete_value() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let source = 1000;
        assert!(store.set(0, key, value, version, source, None));
        assert_eq!(store.del(&key, 0), None);
        assert_eq!(store.del(&key, version), Some((value, version, source)));
        assert_eq!(store.get(&key), None);
    }

    /// Must auto clear key after expire
    #[test]
    fn expire_value() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let source = 1000;
        assert!(store.set(0, key, value, version, source, Some(100)));
        assert_eq!(store.get(&key), Some((&value, version, source)));
        assert!(!store.set(50, key, value, version, source, Some(100)));
        store.tick(50);
        assert_eq!(store.get(&key), Some((&value, version, source)));
        store.tick(100);
        assert_eq!(store.get(&key), Some((&value, version, source)));

        //after 100 from 50 => should expire
        store.tick(150);
        assert_eq!(store.get(&key), None);
    }

    /// Must add event after subscribe and set
    #[test]
    fn subscribe() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.set(0, key, value, version, source, None));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, source, handler)));
        assert_eq!(store.poll(), None);

        //subscribe more time should not fire
        store.subscribe(0, &key, handler, None);
        assert_eq!(store.poll(), None);

        //set with same version should not fire
        assert!(!store.set(0, key, value, version, source, None));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and set
    #[test]
    fn subscribe_after_set() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        assert!(store.set(0, key, value, version, source, None));
        assert_eq!(store.poll(), None);
        store.subscribe(0, &key, handler, None);
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must add multi event after subscribe multi handlers and set
    #[test]
    fn subscribe_multi_handlers() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler1 = 1;
        let handler2 = 2;
        let source = 1000;
        store.subscribe(0, &key, handler1, None);
        store.subscribe(0, &key, handler2, None);
        assert!(store.set(0, key, value, version, source, None));
        let event1 = store.poll().expect("Should return NotifySet");
        let event2 = store.poll().expect("Should return NotifySet");

        let expected1 = OutputEvent::NotifySet(key, value, version, source, handler1);
        let expected2 = OutputEvent::NotifySet(key, value, version, source, handler2);

        assert!(event1 == expected1 || event1 == expected2);
        assert!(event2 == expected1 || event2 == expected2);
        assert_ne!(event1, event2);

        assert_eq!(store.poll(), None);
    }

    /// Must not add event after unsubscribe and set
    #[test]
    fn unsubscribe() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.unsubscribe(&key, &handler));
        assert!(store.set(0, key, value, version, source, None));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and del
    #[test]
    fn subscribe_del() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.set(0, key, value, version, source, None));
        assert_eq!(store.del(&key, version), Some((value, version, source)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, source, handler)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must not add event after unsubscribe and del
    #[test]
    fn unsubscribe_del() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.unsubscribe(&key, &handler));
        assert!(store.set(0, key, value, version, source, None));
        assert_eq!(store.del(&key, version), Some((value, version, source)));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and data expire
    #[test]
    fn subscribe_expire() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.set(0, key, value, version, source, Some(100)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
        store.tick(100);
        assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must clear handler after expire
    #[test]
    fn expire_handler() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let key = 1;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, Some(100));
        assert!(store.set(0, key, value, version, source, None));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, value, version, source, handler)));
        store.tick(100);
        assert_eq!(store.del(&key, version), Some((value, version, source)));
        assert_eq!(store.poll(), None);
    }

    /// Should clear memory after delete
    #[test]
    fn delete_memory() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();
        let info = allocation_counter::measure(|| {
            let key1 = 1;
            let value1 = 2;
            let version1 = 1;
            let source1 = 1000;
            assert!(store.set(0, key1, value1, version1, source1, None));
            assert_eq!(store.del(&key1, version1), Some((value1, version1, source1)));
            assert_eq!(store.keys.len(), 0);
            assert_eq!(store.events.len(), 0);
        });
        assert_eq!(info.count_current, 0);
    }

    /// Should clear memory after unsubscribe
    #[test]
    fn unsubscribe_memory() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();

        let key1 = 1;
        let handler1 = 1;

        let info = allocation_counter::measure(|| {
            store.subscribe(0, &key1, handler1, None);
            assert!(store.unsubscribe(&key1, &handler1));
        });
        assert_eq!(info.count_current, 0);
    }

    /// Should clear memory after expire data
    #[test]
    fn expire_memory() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();

        let key1 = 1;
        let value1 = 2;
        let version1 = 1;
        let source1 = 1000;

        let info = allocation_counter::measure(|| {
            assert!(store.set(0, key1, value1, version1, source1, Some(100)));
            store.tick(100);
        });
        assert_eq!(info.count_current, 0);
    }

    /// Should clear memory after expire handler
    #[test]
    fn expire_handler_memory() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();

        let key1 = 1;
        let handler1 = 1;

        let info = allocation_counter::measure(|| {
            store.subscribe(0, &key1, handler1, Some(100));
            store.tick(100);
        });
        assert_eq!(info.count_current, 0);
    }

    /// Should clear memory after pop all events
    #[test]
    fn pop_all_memory() {
        let mut store = SimpleKeyValue::<u32, u32, u32, u32>::new();

        let key1 = 1;
        let value1 = 2;
        let version1 = 1;
        let handler1 = 1;
        let source1 = 1000;
        store.subscribe(0, &key1, handler1, None);

        let info = allocation_counter::measure(|| {
            assert!(store.set(0, key1, value1, version1, source1, None));
            assert_eq!(store.del(&key1, version1), Some((value1, version1, source1)));

            assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key1, value1, version1, source1, handler1)));
            assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key1, value1, version1, source1, handler1)));
            assert_eq!(store.poll(), None);
        });
        assert_eq!(info.count_current, 0);
    }
}

use atm0s_sdn_utils::hashmap::HashMap;
use atm0s_sdn_utils::vec_dequeue::VecDeque;
use std::collections::hash_map;
use std::hash::Hash;

#[derive(Eq, PartialEq, Debug)]
pub enum OutputEvent<Key, SubKey, Value, Source, Handler> {
    NotifySet(Key, SubKey, Value, u64, Source, Handler),
    NotifyDel(Key, SubKey, Value, u64, Source, Handler),
}

struct HandlerSlot {
    #[allow(dead_code)]
    added_at: u64,
    expire_at: Option<u64>,
}

struct ValueSlot<Value, Source> {
    value: Option<(Value, u64, Source)>,
    expire_at: Option<u64>,
}

struct HashSlot<SubKey, Value, Source, Handler> {
    keys: HashMap<SubKey, ValueSlot<Value, Source>>,
    listeners: HashMap<Handler, HandlerSlot>,
}

pub struct HashmapKeyValue<Key, SubKey, Value, Source, Handlder> {
    maps: HashMap<Key, HashSlot<SubKey, Value, Source, Handlder>>,
    events: VecDeque<OutputEvent<Key, SubKey, Value, Source, Handlder>>,
}

impl<Key, SubKey, Value, Source, Handler> HashmapKeyValue<Key, SubKey, Value, Source, Handler>
where
    Key: PartialEq + Eq + Hash + Clone,
    SubKey: PartialEq + Eq + Hash + Clone,
    Value: Clone,
    Source: Clone,
    Handler: PartialEq + Eq + Hash + Clone,
{
    pub fn new() -> Self {
        Self {
            maps: Default::default(),
            events: Default::default(),
        }
    }

    #[allow(unused)]
    pub(crate) fn len(&self) -> usize {
        self.maps.len()
    }

    /// This function is call in each tick miliseconds, this function will check all expire time
    /// and clear all expired data, and fire all expire events
    /// It also clear expired handlers
    pub fn tick(&mut self, now: u64) {
        // Set value of key to None if it is expired and fire del event
        let mut expired_keys = vec![];
        for (key, map) in self.maps.iter() {
            for (sub_key, slot) in map.keys.iter() {
                if let Some(expire_at) = slot.expire_at {
                    if expire_at <= now {
                        if let Some((_, version, _)) = &slot.value {
                            expired_keys.push((key.clone(), sub_key.clone(), *version));
                        }
                    }
                }
            }
        }
        for (key, sub_key, version) in expired_keys {
            self.del(&key, &sub_key, version);
        }

        // Clear expired handlers
        for (_, map) in self.maps.iter_mut() {
            let mut expired_handlers = vec![];
            for (handler, handler_slot) in map.listeners.iter() {
                if let Some(expire_at) = handler_slot.expire_at {
                    if expire_at <= now {
                        expired_handlers.push(handler.clone());
                    }
                }
            }
            for handler in expired_handlers {
                map.listeners.remove(&handler);
            }
        }

        // Clear key if it value is None and no handlers
        let mut empty_keys = vec![];
        for (key, map) in self.maps.iter() {
            if map.keys.is_empty() && map.listeners.is_empty() {
                empty_keys.push(key.clone());
            }
        }
        for key in empty_keys {
            self.maps.remove(&key);
        }
    }

    /// EX seconds -- Set the specified expire time, in seconds.
    /// version -- Version of value, it should be increased each time new data is set
    pub fn set(&mut self, now_ms: u64, key: Key, sub_key: SubKey, value: Value, version: u64, source: Source, ex: Option<u64>) -> bool {
        match self.maps.get_mut(&key) {
            Some(map) => match map.keys.entry(sub_key.clone()) {
                hash_map::Entry::Occupied(mut slot) => {
                    let slot_inner = slot.get_mut();
                    slot_inner.expire_at = ex.map(|ex| now_ms + ex);
                    if let Some((old_value, old_version, old_source)) = &mut slot_inner.value {
                        if *old_version < version {
                            *old_value = value;
                            *old_version = version;
                            *old_source = source;
                            Self::fire_set_events(&key, &sub_key, slot_inner, &map.listeners, &mut self.events);
                            true
                        } else {
                            false
                        }
                    } else {
                        slot_inner.value = Some((value.clone(), version, source.clone()));
                        Self::fire_set_events(&key, &sub_key, slot_inner, &map.listeners, &mut self.events);
                        true
                    }
                }
                hash_map::Entry::Vacant(slot) => {
                    let slot_inner = slot.insert(ValueSlot {
                        value: Some((value, version, source)),
                        expire_at: ex.map(|ex| now_ms + ex),
                    });
                    Self::fire_set_events(&key, &sub_key, slot_inner, &map.listeners, &mut self.events);
                    true
                }
            },
            None => {
                let slot = ValueSlot {
                    value: Some((value, version, source)),
                    expire_at: ex.map(|ex| now_ms + ex),
                };
                let map = HashSlot {
                    keys: HashMap::from([(sub_key, slot)]),
                    listeners: Default::default(),
                };
                self.maps.insert(key, map);
                true
            }
        }
    }

    pub fn get(&self, key: &Key) -> Option<Vec<(SubKey, &Value, u64, Source)>> {
        let map = self.maps.get(key)?;
        if map.keys.is_empty() {
            None
        } else {
            let mut result = vec![];
            for (sub_key, slot) in map.keys.iter() {
                if let Some((value, version, source)) = &slot.value {
                    result.push((sub_key.clone(), value, *version, source.clone()));
                }
            }
            Some(result)
        }
    }

    pub fn del(&mut self, key: &Key, sub_key: &SubKey, request_version: u64) -> Option<(Value, u64, Source)> {
        let map = self.maps.get_mut(key)?;
        if map.keys.is_empty() {
            return None;
        }
        let slot = map.keys.get_mut(sub_key)?;
        if let Some((_, version, _)) = &slot.value {
            if *version > request_version {
                return None;
            }
            Self::fire_del_events(key, sub_key, slot, &map.listeners, &mut self.events);
        } else {
            return None;
        }

        let slot = map.keys.remove(sub_key)?;
        if map.listeners.is_empty() && map.keys.is_empty() {
            self.maps.remove(key);
        }
        slot.value
    }

    pub fn subscribe(&mut self, now_ms: u64, key: &Key, handler_uuid: Handler, ex: Option<u64>) {
        match self.maps.get_mut(key) {
            Some(slot) => match slot.listeners.entry(handler_uuid.clone()) {
                hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().expire_at = ex.map(|ex| now_ms + ex);
                }
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(HandlerSlot {
                        added_at: now_ms,
                        expire_at: ex.map(|ex| now_ms + ex),
                    });
                    for (sub_key, slot) in slot.keys.iter() {
                        if let Some((value, version, source)) = &slot.value {
                            self.events
                                .push_back(OutputEvent::NotifySet(key.clone(), sub_key.clone(), value.clone(), *version, source.clone(), handler_uuid.clone()));
                        }
                    }
                }
            },
            None => {
                self.maps.insert(
                    key.clone(),
                    HashSlot {
                        keys: HashMap::new(),
                        listeners: HashMap::from([(
                            handler_uuid,
                            HandlerSlot {
                                added_at: now_ms,
                                expire_at: ex.map(|ex| now_ms + ex),
                            },
                        )]),
                    },
                );
            }
        }
    }

    pub fn unsubscribe(&mut self, key: &Key, handler_uuid: &Handler) -> bool {
        match self.maps.get_mut(key) {
            None => false,
            Some(map) => {
                if map.listeners.remove(handler_uuid).is_some() {
                    if map.keys.is_empty() && map.listeners.is_empty() {
                        self.maps.remove(key);
                    }
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn poll(&mut self) -> Option<OutputEvent<Key, SubKey, Value, Source, Handler>> {
        self.events.pop_front()
    }

    //Private functions
    ///Fire set events of slot, this should be called after new data is set
    fn fire_set_events(
        key: &Key,
        sub_key: &SubKey,
        slot: &ValueSlot<Value, Source>,
        listeners: &HashMap<Handler, HandlerSlot>,
        events: &mut VecDeque<OutputEvent<Key, SubKey, Value, Source, Handler>>,
    ) -> bool {
        if let Some((value, version, source)) = &slot.value {
            for (handler, _handler_slot) in listeners.iter() {
                events.push_back(OutputEvent::NotifySet(key.clone(), sub_key.clone(), value.clone(), *version, source.clone(), handler.clone()));
            }
            true
        } else {
            false
        }
    }

    ///Fire del events of slot, this should be called before delete data
    fn fire_del_events(
        key: &Key,
        sub_key: &SubKey,
        slot: &ValueSlot<Value, Source>,
        listeners: &HashMap<Handler, HandlerSlot>,
        events: &mut VecDeque<OutputEvent<Key, SubKey, Value, Source, Handler>>,
    ) -> bool {
        if let Some((value, version, source)) = &slot.value {
            for (handler, _handler_slot) in listeners.iter() {
                events.push_back(OutputEvent::NotifyDel(key.clone(), sub_key.clone(), value.clone(), *version, source.clone(), handler.clone()));
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
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let source = 1000;
        assert!(store.set(0, key, sub_key, value, source, version, None));
        assert!(!store.set(0, key, sub_key, value, source, version, None));
        assert_eq!(store.get(&key), Some(vec![(sub_key, &value, source, version)]));
    }

    /// Must return None after delete
    #[test]
    fn delete_value() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let source = 1000;
        assert!(store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.del(&key, &sub_key, 0), None);
        assert_eq!(store.del(&key, &sub_key, version), Some((value, version, source)));
        assert_eq!(store.get(&key), None);
    }

    /// Must return None after delete
    #[test]
    fn delete_value_remain_other() {
        let mut store = HashmapKeyValue::<u32, u64, u32, u32, u32>::new();
        let key = 1;
        let sub_key1 = 11;
        let sub_key2 = 12;
        let value1 = 2;
        let value2 = 3;
        let version = 1;
        let source = 1000;
        assert!(store.set(0, key, sub_key1, value1, version, source, None));
        assert!(store.set(0, key, sub_key2, value2, version, source, None));
        assert_eq!(store.del(&key, &sub_key1, 0), None);
        assert_eq!(store.del(&key, &sub_key1, version), Some((value1, version, source)));
        assert_eq!(store.get(&key), Some(vec![(sub_key2, &value2, version, source)]));
    }

    /// Must auto clear key after expire
    #[test]
    fn expire_value() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let source = 1000;
        assert!(store.set(0, key, sub_key, value, version, source, Some(100)));
        assert!(!store.set(50, key, sub_key, value, version, source, Some(100)));
        store.tick(50);
        assert_eq!(store.get(&key), Some(vec![(sub_key, &value, version, source)]));
        store.tick(100);
        assert_eq!(store.get(&key), Some(vec![(sub_key, &value, version, source)]));

        //after 100 from 50 => should expire
        store.tick(150);
        assert_eq!(store.get(&key), None);
    }

    /// Must add event after subscribe and set
    #[test]
    fn subscribe() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, sub_key, value, version, source, handler)));
        assert_eq!(store.poll(), None);

        //subscribe more time should not fire
        store.subscribe(0, &key, handler, None);
        assert_eq!(store.poll(), None);

        //set with same version should not fire
        assert!(!store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and set
    #[test]
    fn subscribe_after_set() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        assert!(store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.poll(), None);
        store.subscribe(0, &key, handler, None);
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, sub_key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must add multi event after subscribe multi handlers and set
    #[test]
    fn subscribe_multi_handlers() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler1 = 1;
        let handler2 = 2;
        let source = 1000;
        store.subscribe(0, &key, handler1, None);
        store.subscribe(0, &key, handler2, None);
        assert!(store.set(0, key, sub_key, value, version, source, None));
        let event1 = store.poll().expect("Should return NotifySet");
        let event2 = store.poll().expect("Should return NotifySet");

        let expected1 = OutputEvent::NotifySet(key, sub_key, value, version, source, handler1);
        let expected2 = OutputEvent::NotifySet(key, sub_key, value, version, source, handler2);

        assert!(event1 == expected1 || event1 == expected2);
        assert!(event2 == expected1 || event2 == expected2);
        assert_ne!(event1, event2);

        assert_eq!(store.poll(), None);
    }

    /// Must not add event after unsubscribe and set
    #[test]
    fn unsubscribe() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.unsubscribe(&key, &handler));
        assert!(store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and del
    #[test]
    fn subscribe_del() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.del(&key, &sub_key, version), Some((value, version, source)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, sub_key, value, version, source, handler)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key, sub_key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must not add event after unsubscribe and del
    #[test]
    fn unsubscribe_del() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.unsubscribe(&key, &handler));
        assert!(store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.del(&key, &sub_key, version), Some((value, version, source)));
        assert_eq!(store.poll(), None);
    }

    /// Must add event after subscribe and data expire
    #[test]
    fn subscribe_expire() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, None);
        assert!(store.set(0, key, sub_key, value, version, source, Some(100)));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, sub_key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
        store.tick(100);
        assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key, sub_key, value, version, source, handler)));
        assert_eq!(store.poll(), None);
    }

    /// Must clear handler after expire
    #[test]
    fn expire_handler() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();
        let key = 1;
        let sub_key = 11;
        let value = 2;
        let version = 1;
        let handler = 1;
        let source = 1000;
        store.subscribe(0, &key, handler, Some(100));
        assert!(store.set(0, key, sub_key, value, version, source, None));
        assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key, sub_key, value, version, source, handler)));
        store.tick(100);
        assert_eq!(store.del(&key, &sub_key, version), Some((value, version, source)));
        assert_eq!(store.poll(), None);
    }

    /// Should clear memory after delete
    #[test]
    fn delete_memory() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();

        let key1 = 1;
        let sub_key1 = 11;
        let value1 = 2;
        let version1 = 1;
        let source1 = 1000;

        let info = allocation_counter::measure(|| {
            assert!(store.set(0, key1, sub_key1, value1, version1, source1, None));
            assert_eq!(store.del(&key1, &sub_key1, version1), Some((value1, version1, source1)));
            assert_eq!(store.maps.len(), 0);
            assert_eq!(store.events.len(), 0);
        });
        assert_eq!(info.count_current, 0);
    }

    /// Should clear memory after unsubscribe
    #[test]
    fn unsubscribe_memory() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();

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
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();

        let key1 = 1;
        let sub_key1 = 11;
        let value1 = 2;
        let version1 = 1;
        let source1 = 1000;

        let info = allocation_counter::measure(|| {
            assert!(store.set(0, key1, sub_key1, value1, version1, source1, Some(100)));
            store.tick(100);
        });
        assert_eq!(info.count_current, 0);
    }

    /// Should clear memory after expire handler
    #[test]
    fn expire_handler_memory() {
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();

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
        let mut store = HashmapKeyValue::<u32, u32, u32, u32, u32>::new();

        let key1 = 1;
        let sub_key1 = 11;
        let value1 = 2;
        let version1 = 1;
        let handler1 = 1;
        let source1 = 1000;
        store.subscribe(0, &key1, handler1, None);

        let info = allocation_counter::measure(|| {
            assert!(store.set(0, key1, sub_key1, value1, version1, source1, None));
            assert_eq!(store.del(&key1, &sub_key1, version1), Some((value1, version1, source1)));

            assert_eq!(store.poll(), Some(OutputEvent::NotifySet(key1, sub_key1, value1, version1, source1, handler1)));
            assert_eq!(store.poll(), Some(OutputEvent::NotifyDel(key1, sub_key1, value1, version1, source1, handler1)));
            assert_eq!(store.poll(), None);
        });
        assert_eq!(info.count_current, 0);
    }
}

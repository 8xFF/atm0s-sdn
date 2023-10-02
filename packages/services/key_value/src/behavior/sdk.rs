use std::sync::Arc;

use parking_lot::RwLock;

use super::simple_local::LocalStorage;

pub struct KeyValueSdk {
    local: Arc<RwLock<LocalStorage>>,
}

impl KeyValueSdk {
    pub fn new(local: Arc<RwLock<LocalStorage>>) -> Self {
        Self { local }
    }

    pub fn set(&mut self, key: String, value: Vec<u8>, version: u64, ex: Option<u64>) -> bool {
        false
    }

    pub fn get(&self, key: &String) -> Option<(Vec<u8>, u64)> {
        None
    }

    pub fn del(&mut self, key: &String) {}

    pub fn subscribe(&mut self, key: &String, from: u64, ex: Option<u64>) {}

    pub fn unsubscribe(&mut self, key: &String, from: &u64) {}
}

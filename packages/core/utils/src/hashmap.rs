use std::collections::HashMap as HashMapOrigin;

pub struct HashMap<K, V> {
    data: HashMapOrigin<K, V>,
}

impl<K, V> Default for HashMap<K, V>
where
    K: std::cmp::Eq + std::hash::Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

///implement HashMapAutoFree mirror all function from HashMap, and auto call shink_to_fit when delete
impl<K, V> HashMap<K, V>
where
    K: std::cmp::Eq + std::hash::Hash,
{
    pub fn new() -> Self {
        HashMap { data: HashMapOrigin::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        HashMap {
            data: HashMapOrigin::with_capacity(capacity),
        }
    }

    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional)
    }

    pub fn shrink_to_fit(&mut self) {
        self.data.shrink_to_fit()
    }

    pub fn clear(&mut self) {
        self.data.clear();
        #[cfg(feature = "auto-clear")]
        self.data.shrink_to_fit();
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.data.insert(key, value)
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let ret = self.data.remove(key);
        #[cfg(feature = "auto-clear")]
        self.shrink_to_fit();
        ret
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.data.get(key)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.data.get_mut(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.data.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> std::collections::hash_map::Iter<K, V> {
        self.data.iter()
    }

    pub fn iter_mut(&mut self) -> std::collections::hash_map::IterMut<K, V> {
        self.data.iter_mut()
    }

    pub fn keys(&self) -> std::collections::hash_map::Keys<K, V> {
        self.data.keys()
    }

    pub fn values(&self) -> std::collections::hash_map::Values<K, V> {
        self.data.values()
    }

    pub fn values_mut(&mut self) -> std::collections::hash_map::ValuesMut<K, V> {
        self.data.values_mut()
    }

    // pub fn drain(&mut self) -> std::collections::hash_map::Drain<K, V> {
    //     self.data.drain()
    //     //TODO shink_to_fit
    // }

    pub fn entry(&mut self, key: K) -> std::collections::hash_map::Entry<K, V> {
        self.data.entry(key)
    }

    pub fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        HashMap { data: HashMapOrigin::from_iter(iter) }
    }
}

impl<K, V, const N: usize> From<[(K, V); N]> for HashMap<K, V>
where
    K: std::cmp::Eq + std::hash::Hash,
{
    fn from(arr: [(K, V); N]) -> Self {
        Self::from_iter(arr)
    }
}

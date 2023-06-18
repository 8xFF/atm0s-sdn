use std::collections::VecDeque as VecDequeOrigin;

pub struct VecDeque<T> {
    data: VecDequeOrigin<T>,
}

/// Implement mirror function of VecDeque and auto call shink_to_fit when delete
impl<T> VecDeque<T> {
    pub fn new() -> Self {
        VecDeque { data: VecDequeOrigin::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        VecDeque {
            data: VecDequeOrigin::with_capacity(capacity),
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

    pub fn push_back(&mut self, value: T) {
        self.data.push_back(value)
    }

    pub fn pop_front(&mut self) -> Option<T> {
        let ret = self.data.pop_front();
        #[cfg(feature = "auto-clear")]
        self.shrink_to_fit();
        ret
    }

    pub fn pop_back(&mut self) -> Option<T> {
        let ret = self.data.pop_back();
        #[cfg(feature = "auto-clear")]
        self.shrink_to_fit();
        ret
    }

    pub fn front(&self) -> Option<&T> {
        self.data.front()
    }

    pub fn back(&self) -> Option<&T> {
        self.data.back()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

///implement default
impl<T> Default for VecDeque<T> {
    fn default() -> Self {
        Self::new()
    }
}

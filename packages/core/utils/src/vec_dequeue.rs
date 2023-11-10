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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut deque = VecDeque::new();
        deque.push_back(1);
        deque.push_back(2);
        deque.push_back(3);
        assert_eq!(deque.pop_front(), Some(1));
        assert_eq!(deque.pop_front(), Some(2));
        assert_eq!(deque.pop_front(), Some(3));
        assert_eq!(deque.pop_front(), None);
    }

    #[test]
    fn test_front_back() {
        let mut deque = VecDeque::new();
        deque.push_back(1);
        deque.push_back(2);
        deque.push_back(3);
        assert_eq!(deque.front(), Some(&1));
        assert_eq!(deque.back(), Some(&3));
    }

    #[test]
    fn test_clear() {
        let mut deque = VecDeque::new();
        deque.push_back(1);
        deque.push_back(2);
        deque.push_back(3);
        deque.clear();
        assert_eq!(deque.len(), 0);
        assert_eq!(deque.capacity(), 0);
    }

    #[test]
    fn test_reserve() {
        let mut deque = VecDeque::<u32>::new();
        deque.reserve(10);
        assert!(deque.capacity() >= 10);
    }

    #[test]
    fn test_shrink_to_fit() {
        let mut deque = VecDeque::with_capacity(10);
        deque.push_back(1);
        deque.push_back(2);
        deque.push_back(3);
        deque.pop_front();
        deque.shrink_to_fit();
        assert_eq!(deque.capacity(), 2);
    }

    #[test]
    fn test_default() {
        let deque: VecDeque<i32> = VecDeque::default();
        assert_eq!(deque.len(), 0);
        assert_eq!(deque.capacity(), 0);
    }
}

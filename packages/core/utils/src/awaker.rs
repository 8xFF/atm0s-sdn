use std::sync::{atomic::AtomicUsize, Arc};

pub trait Awaker: Send + Sync {
    fn notify(&self);
    fn pop_awake_count(&self) -> usize;
}

#[derive(Default)]
pub struct MockAwaker {
    atomic: Arc<AtomicUsize>,
}

impl Awaker for MockAwaker {
    fn notify(&self) {
        self.atomic.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn pop_awake_count(&self) -> usize {
        self.atomic.swap(0, std::sync::atomic::Ordering::Relaxed)
    }
}

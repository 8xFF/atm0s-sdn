use std::sync::{atomic::AtomicUsize, Arc};

#[async_trait::async_trait]
pub trait Awaker: Send + Sync {
    fn notify(&self);
    fn pop_awake_count(&self) -> usize;
    async fn wait(&self);
}

#[derive(Default)]
pub struct MockAwaker {
    atomic: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Awaker for MockAwaker {
    fn notify(&self) {
        self.atomic.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn pop_awake_count(&self) -> usize {
        self.atomic.swap(0, std::sync::atomic::Ordering::Relaxed)
    }

    async fn wait(&self) {
        panic!("Should not called")
    }
}

#[derive(Default)]
pub struct AsyncAwaker {
    notify: Arc<async_notify::Notify>,
}

#[async_trait::async_trait]
impl Awaker for AsyncAwaker {
    fn notify(&self) {
        self.notify.notify();
    }

    fn pop_awake_count(&self) -> usize {
        panic!("Should not called")
    }

    async fn wait(&self) {
        self.notify.notified().await;
    }
}

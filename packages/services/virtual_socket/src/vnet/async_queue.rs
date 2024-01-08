use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use atm0s_sdn_utils::vec_dequeue::VecDeque;
use futures::Future;
use parking_lot::{Mutex, RwLock};

#[derive(Clone)]
pub struct AsyncQueue<T> {
    data: Arc<RwLock<VecDeque<T>>>,
    awake: Arc<Mutex<Option<Waker>>>,
    max_size: usize,
}

impl<T> AsyncQueue<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: Default::default(),
            awake: Default::default(),
            max_size,
        }
    }

    pub fn try_push(&self, item: T) -> Result<(), T> {
        let mut data = self.data.write();
        if data.len() >= self.max_size {
            return Err(item);
        }
        data.push_back(item);
        if data.len() == 1 {
            if let Some(waker) = self.awake.lock().take() {
                waker.wake();
            }
        }
        Ok(())
    }

    pub fn try_pop(&self) -> Option<T> {
        let mut data = self.data.write();
        data.pop_front()
    }

    pub fn poll_pop(&self, cx: &mut std::task::Context) -> std::task::Poll<Option<T>> {
        let mut data = self.data.write();
        if let Some(item) = data.pop_front() {
            return std::task::Poll::Ready(Some(item));
        }
        *self.awake.lock() = Some(cx.waker().clone());
        std::task::Poll::Pending
    }

    pub fn recv(&self) -> Recv<'_, T> {
        Recv { queue: self }
    }
}

pub struct Recv<'a, T> {
    queue: &'a AsyncQueue<T>,
}

impl<'a, T> Future for Recv<'a, T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.queue.poll_pop(cx)
    }
}

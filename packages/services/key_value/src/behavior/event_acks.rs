use std::collections::{HashMap, VecDeque};

use crate::ReqId;

pub const RESEND_AFTER_MS: u64 = 500;

pub struct EventAckManager<T: Clone> {
    events: HashMap<ReqId, (u64, T, u8)>,
    actions: VecDeque<T>,
}

impl<T> EventAckManager<T>
where
    T: Clone,
{
    pub fn new() -> Self {
        Self {
            events: Default::default(),
            actions: Default::default(),
        }
    }

    // add to queue and auto retry each tick
    pub fn add_event(&mut self, now_ms: u64, req_id: ReqId, event: T, retry_count: u8) {
        log::debug!("[EventAckManager] Add event with req_id {}, retry_count {}", req_id, retry_count);
        self.actions.push_back(event.clone());
        self.events.insert(req_id, (now_ms, event, retry_count));
    }

    pub fn on_ack(&mut self, req_id: ReqId) {
        log::debug!("[EventAckManager] On ack with req_id {}", req_id);
        self.events.remove(&req_id);
    }

    pub fn tick(&mut self, now_ms: u64) {
        for (_req_id, (pushed_ms, event, retry_count)) in self.events.iter_mut() {
            if *retry_count > 0 && now_ms >= *pushed_ms + RESEND_AFTER_MS {
                log::debug!("[EventAckManager] Retry event with req_id {}, retry_count {}", _req_id, *retry_count);
                *retry_count -= 1;
                self.actions.push_back(event.clone());
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<T> {
        self.actions.pop_front()
    }
}

use std::collections::{HashMap, VecDeque};

use crate::ReqId;

pub struct EventAckManager<T: Clone> {
    events: HashMap<ReqId, (T, u8)>,
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
    pub fn add_event(&mut self, req_id: ReqId, event: T, retry_count: u8) {
        log::debug!("[EventAckManager] Add event with req_id {}, retry_count {}", req_id, retry_count);
        self.actions.push_back(event.clone());
        self.events.insert(req_id, (event, retry_count));
    }

    pub fn on_ack(&mut self, req_id: ReqId) {
        log::debug!("[EventAckManager] On ack with req_id {}", req_id);
        self.events.remove(&req_id);
    }

    pub fn tick(&mut self) {
        for (_req_id, (event, retry_count)) in self.events.iter_mut() {
            if *retry_count > 0 {
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

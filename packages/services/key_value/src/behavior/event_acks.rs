use std::collections::HashMap;

use crate::ReqId;

pub struct EventAckManager<T: Clone> {
    events: HashMap<ReqId, (T, u8)>,
    actions: Vec<T>,
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
        self.actions.push(event.clone());
        self.events.insert(req_id, (event, retry_count));
    }

    pub fn on_ack(&mut self, req_id: ReqId) {
        self.events.remove(&req_id);
    }

    pub fn tick(&mut self) {
        for (_req_id, (event, retry_count)) in self.events.iter_mut() {
            if *retry_count > 0 {
                *retry_count -= 1;
                self.actions.push(event.clone());
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<T> {
        self.actions.pop()
    }
}

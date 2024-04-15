use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::AtomicU64,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::shadow::ShadowRouterHistory;
use parking_lot::Mutex;

const HISTORY_TIMEOUT_MS: u64 = 2000;

#[derive(Debug, Default)]
pub struct DataWorkerHistory {
    now_ms: AtomicU64,
    queue: Mutex<VecDeque<(u64, (Option<NodeId>, u8, u16))>>,
    map: Mutex<HashMap<(Option<NodeId>, u8, u16), bool>>,
}

impl ShadowRouterHistory for DataWorkerHistory {
    fn already_received_broadcast(&self, from: Option<NodeId>, service: u8, seq: u16) -> bool {
        let mut map = self.map.lock();
        let mut queue = self.queue.lock();
        let now_ms = self.now_ms.load(std::sync::atomic::Ordering::Relaxed);
        if map.contains_key(&(from, service, seq)) {
            return true;
        }

        map.insert((from, service, seq), true);
        queue.push_back((now_ms, (from, service, seq)));
        if queue.len() > 10000 {
            let (_ts, pair) = queue.pop_front().expect("queue should not empty");
            map.remove(&pair);
        }
        false
    }

    fn set_ts(&self, now_ms: u64) {
        self.now_ms.store(now_ms, std::sync::atomic::Ordering::Relaxed);
        let mut map = self.map.lock();
        let mut queue = self.queue.lock();

        while let Some((time, key)) = queue.front() {
            if now_ms >= *time + HISTORY_TIMEOUT_MS {
                map.remove(key);
                queue.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use atm0s_sdn_router::shadow::ShadowRouterHistory;

    use crate::history::HISTORY_TIMEOUT_MS;

    use super::DataWorkerHistory;

    #[test]
    fn simple_work() {
        let history = DataWorkerHistory::default();

        assert_eq!(history.already_received_broadcast(Some(1), 1, 1), false);
        assert_eq!(history.already_received_broadcast(Some(1), 1, 1), true);

        //after timeout
        history.set_ts(HISTORY_TIMEOUT_MS);
        assert_eq!(history.already_received_broadcast(Some(1), 1, 1), false);
    }
}

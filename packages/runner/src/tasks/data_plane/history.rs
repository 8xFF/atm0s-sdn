use std::collections::HashMap;

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::shadow::ShadowRouterHistory;
use parking_lot::Mutex;

#[derive(Debug, Default)]
pub struct DataWorkerHistory {
    queue: Mutex<Vec<(Option<NodeId>, u8, u16)>>,
    map: Mutex<HashMap<(Option<NodeId>, u8, u16), bool>>,
}

impl ShadowRouterHistory for DataWorkerHistory {
    fn already_received_broadcast(&self, from: Option<NodeId>, service: u8, seq: u16) -> bool {
        let mut map = self.map.lock();
        let mut queue = self.queue.lock();
        if map.contains_key(&(from, service, seq)) {
            return true;
        }
        map.insert((from, service, seq), true);
        if queue.len() > 10000 {
            let pair = queue.remove(0);
            map.remove(&pair);
        }
        false
    }
}

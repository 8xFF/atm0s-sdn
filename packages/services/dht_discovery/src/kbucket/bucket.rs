use crate::kbucket::entry::{Entry, EntryState};
use crate::kbucket::K_BUCKET;
use bluesea_identity::{NodeAddr, NodeId};

pub struct KBucket {
    distance_bits: usize,
    slots: [Entry; K_BUCKET],
}

impl KBucket {
    pub(crate) fn new(distance_bits: usize) -> Self {
        Self {
            distance_bits,
            slots: [Entry::new(), Entry::new(), Entry::new(), Entry::new()],
        }
    }

    fn sort(&mut self) {
        self.slots.sort_by_key(|e| match e.state() {
            EntryState::Empty => u32::MAX,
            EntryState::Connecting { distance, .. } => *distance,
            EntryState::Connected { distance, .. } => *distance,
        })
    }

    /// Checking if has empty slot
    pub fn has_empty(&self) -> Option<usize> {
        for i in 0..self.slots.len() {
            if self.slots[i].is_empty() {
                return Some(i);
            }
        }
        return None;
    }

    pub fn size(&self) -> u8 {
        let mut size = 0;
        for i in 0..self.slots.len() {
            if !self.slots[i].is_empty() {
                size += 1;
            }
        }
        size
    }

    pub fn connected_size(&self) -> u8 {
        let mut size = 0;
        for i in 0..self.slots.len() {
            if !self.slots[i].is_connected() {
                size += 1;
            }
        }
        size
    }

    pub fn get_node(&self, new_distance: NodeId) -> Option<&EntryState> {
        for slot in &self.slots {
            let state = slot.state();
            match state {
                EntryState::Connecting { distance, .. } => {
                    if *distance == new_distance {
                        return Some(state);
                    }
                }
                EntryState::Connected { distance, .. } => {
                    if *distance == new_distance {
                        return Some(state);
                    }
                }
                EntryState::Empty => {}
            }
        }
        None
    }

    pub fn add_node_connecting(&mut self, new_distance: NodeId, addr: NodeAddr) -> bool {
        for slot in &self.slots {
            match slot.state() {
                EntryState::Connecting { distance, .. } => {
                    if *distance == new_distance {
                        return false;
                    }
                }
                EntryState::Connected { distance, .. } => {
                    if *distance == new_distance {
                        return false;
                    }
                }
                _ => {}
            }
        }
        if let Some(slot) = self.has_empty() {
            //TODO fill timestamp
            self.slots[slot].switch_state(EntryState::Connecting {
                distance: new_distance,
                addr,
                started_at: 0,
            });
            self.sort();
            true
        } else {
            false
        }
    }

    pub fn add_node_connected(&mut self, new_distance: NodeId, addr: NodeAddr) -> bool {
        for slot in &mut self.slots {
            match slot.state() {
                EntryState::Connecting { distance, .. } => {
                    if *distance == new_distance {
                        slot.switch_state(EntryState::Connected {
                            distance: new_distance,
                            addr,
                            started_at: 0,
                        });
                        self.sort();
                        return true;
                    }
                }
                EntryState::Connected { distance, .. } => {
                    if *distance == new_distance {
                        return false;
                    }
                }
                _ => {}
            }
        }
        if let Some(slot) = self.has_empty() {
            //TODO fill timestamp
            self.slots[slot].switch_state(EntryState::Connected {
                distance: new_distance,
                addr,
                started_at: 0,
            });
            self.sort();
            true
        } else {
            false
        }
    }

    pub fn remove_connecting_node(&mut self, new_distance: NodeId) -> bool {
        for slot in &mut self.slots {
            match slot.state() {
                EntryState::Connecting { distance, .. } => {
                    if *distance == new_distance {
                        slot.switch_state(EntryState::Empty);
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    pub fn remove_connected_node(&mut self, new_distance: NodeId) -> bool {
        for slot in &mut self.slots {
            match slot.state() {
                EntryState::Connected { distance, .. } => {
                    if *distance == new_distance {
                        slot.switch_state(EntryState::Empty);
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    pub fn remove_timeout_nodes(&mut self) -> Option<Vec<NodeId>> {
        //TODO
        None
    }

    pub fn nodes(&self) -> Vec<(NodeId, NodeAddr, bool)> {
        let mut res = vec![];
        for slot in &self.slots {
            match slot.state() {
                EntryState::Connected { distance, addr, .. } => {
                    res.push((*distance, addr.clone(), true));
                }
                EntryState::Connecting { distance, addr, .. } => {
                    res.push((*distance, addr.clone(), false));
                }
                _ => {}
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use crate::kbucket::bucket::KBucket;
    use bluesea_identity::{NodeAddr, Protocol};

    #[test]
    fn simple_add_get() {
        let mut bucket = KBucket::new(0);
        assert_eq!(bucket.add_node_connecting(1, NodeAddr::from(Protocol::Udp(1))), true);
        assert_eq!(bucket.add_node_connecting(1, NodeAddr::from(Protocol::Udp(1))), false);
        assert_eq!(bucket.add_node_connected(1, NodeAddr::from(Protocol::Udp(1))), true);

        assert_eq!(bucket.size(), 1);

        assert_eq!(bucket.add_node_connecting(2, NodeAddr::from(Protocol::Udp(2))), true);
        assert_eq!(bucket.nodes(), vec![(1, NodeAddr::from(Protocol::Udp(1)), true), (2, NodeAddr::from(Protocol::Udp(2)), false)]);
    }

    #[test]
    fn remove_connecting() {
        let mut bucket = KBucket::new(0);
        assert_eq!(bucket.add_node_connecting(1, NodeAddr::from(Protocol::Udp(1))), true);
        assert_eq!(bucket.size(), 1);
        assert_eq!(bucket.remove_connecting_node(1), true);
        assert_eq!(bucket.size(), 0);
    }

    #[test]
    fn remove_connected() {
        let mut bucket = KBucket::new(0);
        assert_eq!(bucket.add_node_connected(1, NodeAddr::from(Protocol::Udp(1))), true);
        assert_eq!(bucket.size(), 1);
        assert_eq!(bucket.remove_connected_node(1), true);
        assert_eq!(bucket.size(), 0);
    }

    #[test]
    fn remove_connecting_but_has_connected() {
        let mut bucket = KBucket::new(0);
        assert_eq!(bucket.add_node_connected(1, NodeAddr::from(Protocol::Udp(1))), true);
        assert_eq!(bucket.size(), 1);
        assert_eq!(bucket.remove_connecting_node(1), false);
        assert_eq!(bucket.size(), 1);
    }

    #[test]
    fn remove_connected_but_has_connecting() {
        let mut bucket = KBucket::new(0);
        assert_eq!(bucket.add_node_connecting(1, NodeAddr::from(Protocol::Udp(1))), true);
        assert_eq!(bucket.size(), 1);
        assert_eq!(bucket.remove_connected_node(1), false);
        assert_eq!(bucket.size(), 1);
    }
}

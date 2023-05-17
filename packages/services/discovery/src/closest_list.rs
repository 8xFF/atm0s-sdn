use crate::kbucket::K_BUCKET;
use bluesea_identity::{PeerAddr, PeerId};
use std::collections::{HashMap, VecDeque};

enum PeerState {
    Waiting,
    Connecting,
    Sent,
}

pub struct ClosestList<M> {
    key: PeerId,
    local_peer_id: PeerId,
    peers: Vec<(PeerId, PeerAddr, bool)>,
    msgs: HashMap<PeerId, VecDeque<M>>,
    created_ms: u64,
}

impl<M> ClosestList<M> {
    pub fn new(key: PeerId, local_peer_id: PeerId, created_ms: u64) -> Self {
        Self {
            key,
            local_peer_id,
            peers: Default::default(),
            msgs: Default::default(),
            created_ms,
        }
    }

    pub fn get_key(&self) -> PeerId {
        self.key
    }

    /// This will add peer to list, if is requested => state is Sent
    /// All added peer is sorted by Distance to local_peer_id
    pub fn add_peer(&mut self, peer: PeerId, addr: PeerAddr, requested: bool) {
        for (in_peer, _, _) in &self.peers {
            if *in_peer == peer {
                return;
            }
        }
        self.peers.push((peer, addr, requested));
        let local_peer_id = self.local_peer_id;
        self.peers.sort_by_key(|(peer, _, _)| *peer ^ local_peer_id);
    }

    /// This will pop closest peer which is Waiting state, if not exists it will return None
    pub fn pop_need_connect(&mut self) -> Option<(PeerId, PeerAddr)> {
        for i in 0..K_BUCKET {
            if let Some((peer, addr, requested)) = self.peers.get_mut(i) {
                if !*requested {
                    *requested = true;
                    return Some((*peer, addr.clone()));
                }
            }
        }
        None
    }

    /// This will add message to waiting list for a peer, this list is used when peer is connected
    pub fn add_pending_msg(&mut self, to: PeerId, msg: M) {
        let entry = self.msgs.entry(to).or_insert_with(|| VecDeque::new());
        entry.push_back(msg);
    }

    /// When a peer connected, this function return which message should be send
    pub fn pop_pending_msg(&mut self, to: PeerId) -> Option<M> {
        let msgs = self.msgs.get_mut(&to)?;
        if let Some(msg) = msgs.pop_front() {
            if msgs.is_empty() {
                self.msgs.remove(&to);
            }
            Some(msg)
        } else {
            self.msgs.remove(&to);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::closest_list::ClosestList;

    #[derive(PartialEq, Debug)]
    enum Msg {
        Test,
    }

    #[test]
    fn simple_test() {
        let mut list = ClosestList::<Msg>::new(0, 0, 0);
        list.add_peer(1, "peer1".to_string(), false);
        list.add_peer(2, "peer2".to_string(), false);
        list.add_peer(3, "peer3".to_string(), false);

        assert_eq!(list.pop_need_connect(), Some((1, "peer1".to_string())));
        assert_eq!(list.pop_need_connect(), Some((2, "peer2".to_string())));
        assert_eq!(list.pop_need_connect(), Some((3, "peer3".to_string())));
        assert_eq!(list.pop_need_connect(), None);
    }

    #[test]
    fn test_unordered() {
        let mut list = ClosestList::<Msg>::new(0, 0, 0);
        list.add_peer(2, "peer2".to_string(), false);
        list.add_peer(1, "peer1".to_string(), false);
        list.add_peer(3, "peer3".to_string(), false);

        assert_eq!(list.pop_need_connect(), Some((1, "peer1".to_string())));
        assert_eq!(list.pop_need_connect(), Some((2, "peer2".to_string())));
        assert_eq!(list.pop_need_connect(), Some((3, "peer3".to_string())));
        assert_eq!(list.pop_need_connect(), None);
    }

    #[test]
    fn test_duplicate() {
        let mut list = ClosestList::<Msg>::new(0, 0, 0);
        list.add_peer(2, "peer2".to_string(), false);
        list.add_peer(1, "peer1".to_string(), false);
        list.add_peer(1, "peer1".to_string(), false);
        list.add_peer(3, "peer3".to_string(), false);

        assert_eq!(list.pop_need_connect(), Some((1, "peer1".to_string())));
        assert_eq!(list.pop_need_connect(), Some((2, "peer2".to_string())));
        assert_eq!(list.pop_need_connect(), Some((3, "peer3".to_string())));
        assert_eq!(list.pop_need_connect(), None);
    }

    #[test]
    fn test_requested() {
        let mut list = ClosestList::<Msg>::new(0, 0, 0);
        list.add_peer(2, "peer2".to_string(), true);
        list.add_peer(1, "peer1".to_string(), true);
        list.add_peer(1, "peer1".to_string(), true);
        list.add_peer(3, "peer3".to_string(), true);

        assert_eq!(list.pop_need_connect(), None);
    }

    #[test]
    fn test_pending_msg() {
        let mut list = ClosestList::<Msg>::new(0, 0, 0);
        assert_eq!(list.pop_pending_msg(1), None);

        list.add_pending_msg(1, Msg::Test);
        assert_eq!(list.pop_pending_msg(1), Some(Msg::Test));
    }
}

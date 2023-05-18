use crate::kbucket::K_BUCKET;
use bluesea_identity::{PeerAddr, PeerId};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Eq, PartialEq)]
pub enum FindKeyRequestStatus {
    Requesting,
    Timeout,
    Finished,
}

impl FindKeyRequestStatus {
    pub fn is_timeout(&self) -> bool {
        matches!(self, FindKeyRequestStatus::Timeout)
    }
}

enum PeerState {
    Waiting { at: u64 },
    Connecting { at: u64 },
    Connected { at: u64 },
    Requesting { at: u64 },
    ReceivedAnswer { at: u64 },
    ConnectError { at: u64 },
}

pub struct FindKeyRequest {
    req_id: u32,
    key: PeerId,
    timeout: u64,
    peers: Vec<(PeerId, PeerAddr, PeerState)>,
}

impl FindKeyRequest {
    pub fn new(req_id: u32, key: PeerId, timeout: u64) -> Self {
        Self {
            req_id,
            key,
            timeout,
            peers: Default::default(),
        }
    }

    pub fn is_ended(&self, ts: u64) -> bool {
        match self.status(ts) {
            FindKeyRequestStatus::Requesting => false,
            FindKeyRequestStatus::Timeout => true,
            FindKeyRequestStatus::Finished => true,
        }
    }

    pub fn req_id(&self) -> u32 {
        self.req_id
    }

    pub fn key(&self) -> PeerId {
        self.key
    }

    pub fn status(&self, ts: u64) -> FindKeyRequestStatus {
        let mut waiting_count = 0;
        let mut timeout_count = 0;
        let mut finished_count = 0;
        let loop_len = K_BUCKET.min(self.peers.len());
        for (_, _, state) in &self.peers[0..loop_len] {
            match state {
                PeerState::Waiting { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        timeout_count += 1;
                    }
                }
                PeerState::Connecting { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        timeout_count += 1;
                    }
                }
                PeerState::Connected { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        timeout_count += 1;
                    }
                }
                PeerState::Requesting { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        timeout_count += 1;
                    }
                }
                PeerState::ReceivedAnswer { .. } => {
                    finished_count += 1;
                }
                PeerState::ConnectError { .. } => {
                    timeout_count += 1;
                }
            }
        }

        if waiting_count == 0 && finished_count > 0 {
            FindKeyRequestStatus::Finished
        } else if waiting_count == 0 && finished_count == 0 {
            FindKeyRequestStatus::Timeout
        } else {
            FindKeyRequestStatus::Requesting
        }
    }

    pub fn push_peer(&mut self, ts: u64, peer: PeerId, addr: PeerAddr, connected: bool) {
        for (in_peer, _, _) in &self.peers {
            if *in_peer == peer {
                return;
            }
        }
        let state = if connected {
            PeerState::Connected { at: ts }
        } else {
            PeerState::Waiting { at: ts }
        };
        self.peers.push((peer, addr, state));
        let key = self.key;
        self.peers.sort_by_key(|(peer, _, _)| *peer ^ key);
    }

    pub fn pop_connect(&mut self, ts: u64) -> Option<(PeerId, PeerAddr)> {
        for (peer, addr, state) in &mut self.peers {
            match state {
                PeerState::Waiting { .. } => {
                    *state = PeerState::Connecting { at: ts };
                    return Some((*peer, addr.clone()));
                }
                _ => {}
            }
        }

        None
    }

    pub fn pop_request(&mut self, ts: u64) -> Option<PeerId> {
        for (peer, addr, state) in &mut self.peers {
            match state {
                PeerState::Connected { .. } => {
                    *state = PeerState::Requesting { at: ts };
                    return Some(*peer);
                }
                _ => {}
            }
        }

        None
    }

    pub fn on_connected_peer(&mut self, ts: u64, from_peer: PeerId) -> bool {
        for (peer, _addr, state) in &mut self.peers {
            match state {
                PeerState::Connecting { .. } => {
                    if *peer == from_peer {
                        *state = PeerState::Connected { at: ts };
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    pub fn on_connect_error_peer(&mut self, ts: u64, from_peer: PeerId) -> bool {
        for (peer, addr, state) in &mut self.peers {
            match state {
                PeerState::Connecting { .. } => {
                    if *peer == from_peer {
                        *state = PeerState::ConnectError { at: ts };
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    pub fn on_answered_peer(&mut self, ts: u64, from_peer: PeerId, res: Vec<(PeerId, PeerAddr, bool)>) -> bool {
        for (peer, addr, state) in &mut self.peers {
            match state {
                PeerState::Requesting { .. } => {
                    if *peer == from_peer {
                        *state = PeerState::ReceivedAnswer { at: ts };
                        for (peer, addr, connected) in res {
                            self.push_peer(ts, peer, addr, connected);
                        }
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::PeerAddr;
    use super::{FindKeyRequest, FindKeyRequestStatus};

    #[derive(PartialEq, Debug)]
    enum Msg {
        Test,
    }

    #[test]
    fn test_key() {
        let mut list = FindKeyRequest::new(0, 102, 10000);
        assert_eq!(list.key(), 102);
    }

    #[test]
    fn simple_test_connect() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        list.push_peer(0, 2, PeerAddr::from(Protocol::Udp(2)), false);
        list.push_peer(0, 3, PeerAddr::from(Protocol::Udp(3)), false);

        assert_eq!(list.pop_connect(0), Some((1, PeerAddr::from(Protocol::Udp(1)))));
        assert_eq!(list.pop_connect(0), Some((2, PeerAddr::from(Protocol::Udp(2)))));
        assert_eq!(list.pop_connect(0), Some((3, PeerAddr::from(Protocol::Udp(3)))));
        assert_eq!(list.pop_connect(0), None);
        assert_eq!(list.pop_request(0), None);
    }

    #[test]
    fn test_unordered_connec() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_peer(0, 2, PeerAddr::from(Protocol::Udp(2)), false);
        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        list.push_peer(0, 3, PeerAddr::from(Protocol::Udp(3)), false);

        assert_eq!(list.pop_connect(0), Some((1, PeerAddr::from(Protocol::Udp(1)))));
        assert_eq!(list.pop_connect(0), Some((2, PeerAddr::from(Protocol::Udp(2)))));
        assert_eq!(list.pop_connect(0), Some((3, PeerAddr::from(Protocol::Udp(3)))));
        assert_eq!(list.pop_connect(0), None);
        assert_eq!(list.pop_request(0), None);
    }

    #[test]
    fn simple_test_request() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), true);
        list.push_peer(0, 2, PeerAddr::from(Protocol::Udp(2)), true);
        list.push_peer(0, 3, PeerAddr::from(Protocol::Udp(3)), true);

        assert_eq!(list.pop_request(0), Some(1));
        assert_eq!(list.pop_request(0), Some(2));
        assert_eq!(list.pop_request(0), Some(3));
        assert_eq!(list.pop_request(0), None);
        assert_eq!(list.pop_connect(0), None);
    }

    #[test]
    fn test_unordered_request() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_peer(0, 2, PeerAddr::from(Protocol::Udp(2)), true);
        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), true);
        list.push_peer(0, 3, PeerAddr::from(Protocol::Udp(3)), true);

        assert_eq!(list.pop_request(0), Some(1));
        assert_eq!(list.pop_request(0), Some(2));
        assert_eq!(list.pop_request(0), Some(3));
        assert_eq!(list.pop_request(0), None);
        assert_eq!(list.pop_connect(0), None);
    }

    #[test]
    fn test_duplicate() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        list.push_peer(0, 2, PeerAddr::from(Protocol::Udp(2)), false);
        list.push_peer(0, 3, PeerAddr::from(Protocol::Udp(3)), false);

        assert_eq!(list.pop_connect(0), Some((1, PeerAddr::from(Protocol::Udp(1)))));
        assert_eq!(list.pop_connect(0), Some((2, PeerAddr::from(Protocol::Udp(2)))));
        assert_eq!(list.pop_connect(0), Some((3, PeerAddr::from(Protocol::Udp(3)))));
        assert_eq!(list.pop_connect(0), None);
        assert_eq!(list.pop_request(0), None);
    }

    #[test]
    fn test_timeout_not_connect() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.status(10001), FindKeyRequestStatus::Timeout);
    }

    #[test]
    fn test_connect_error() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        list.pop_connect(0);

        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.on_connect_error_peer(5000, 2), false);
        assert_eq!(list.on_connect_error_peer(5000, 1), true);
        assert_eq!(list.status(10001), FindKeyRequestStatus::Timeout);
    }

    #[test]
    fn test_request_timeout() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        list.pop_connect(0);
        assert_eq!(list.on_connected_peer(5000, 2), false);
        assert_eq!(list.on_connected_peer(5000, 1), true);
        list.pop_request(0);
        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.status(15001), FindKeyRequestStatus::Timeout);
    }

    #[test]
    fn test_request_success() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_peer(0, 1, PeerAddr::from(Protocol::Udp(1)), false);
        list.pop_connect(0);
        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.on_connected_peer(5000, 1), true);
        assert_eq!(list.pop_request(0), Some(1));

        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.on_answered_peer(5000, 1, vec![]), true);
        assert_eq!(list.status(15001), FindKeyRequestStatus::Finished);
    }

    #[test]
    fn test_get_better_result() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_peer(0, 1000, PeerAddr::from(Protocol::Udp(1)), true);
        assert_eq!(list.pop_request(0), Some(1000));
        assert_eq!(list.on_answered_peer(1000, 1000, vec![(100, PeerAddr::from(Protocol::Udp(1)), true)]), true);
        assert_eq!(list.status(1000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.pop_request(1000), Some(100));
        assert_eq!(list.status(1000), FindKeyRequestStatus::Requesting);
    }
}

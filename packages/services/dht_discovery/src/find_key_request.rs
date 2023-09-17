use crate::kbucket::K_BUCKET;
use bluesea_identity::{NodeAddr, NodeId};

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

#[allow(unused)]
enum NodeState {
    Waiting { at: u64 },
    Connecting { at: u64 },
    Connected { at: u64 },
    Requesting { at: u64 },
    ReceivedAnswer { at: u64 },
    ConnectError { at: u64 },
}

pub struct FindKeyRequest {
    req_id: u32,
    key: NodeId,
    timeout: u64,
    nodes: Vec<(NodeId, NodeAddr, NodeState)>,
}

impl FindKeyRequest {
    pub fn new(req_id: u32, key: NodeId, timeout: u64) -> Self {
        Self {
            req_id,
            key,
            timeout,
            nodes: Default::default(),
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

    pub fn key(&self) -> NodeId {
        self.key
    }

    pub fn status(&self, ts: u64) -> FindKeyRequestStatus {
        let mut waiting_count = 0;
        let mut error_count = 0;
        let mut finished_count = 0;
        let loop_len = K_BUCKET.min(self.nodes.len());
        for (_, _, state) in &self.nodes[0..loop_len] {
            match state {
                NodeState::Waiting { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        error_count += 1;
                    }
                }
                NodeState::Connecting { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        error_count += 1;
                    }
                }
                NodeState::Connected { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        error_count += 1;
                    }
                }
                NodeState::Requesting { at, .. } => {
                    if *at + self.timeout > ts {
                        waiting_count += 1;
                    } else {
                        error_count += 1;
                    }
                }
                NodeState::ReceivedAnswer { .. } => {
                    finished_count += 1;
                }
                NodeState::ConnectError { .. } => {
                    error_count += 1;
                }
            }
        }

        if waiting_count == 0 && finished_count > 0 {
            FindKeyRequestStatus::Finished
        } else if waiting_count == 0 && finished_count == 0 && error_count > 0 {
            FindKeyRequestStatus::Timeout
        } else {
            FindKeyRequestStatus::Requesting
        }
    }

    pub fn push_node(&mut self, ts: u64, node: NodeId, addr: NodeAddr, connected: bool) {
        for (in_node, _, _) in &self.nodes {
            if *in_node == node {
                return;
            }
        }
        let state = if connected {
            NodeState::Connected { at: ts }
        } else {
            NodeState::Waiting { at: ts }
        };
        self.nodes.push((node, addr, state));
        let key = self.key;
        self.nodes.sort_by_key(|(node, _, _)| *node ^ key);
    }

    pub fn pop_connect(&mut self, ts: u64) -> Option<(NodeId, NodeAddr)> {
        for (node, addr, state) in &mut self.nodes {
            match state {
                NodeState::Waiting { .. } => {
                    *state = NodeState::Connecting { at: ts };
                    return Some((*node, addr.clone()));
                }
                _ => {}
            }
        }

        None
    }

    pub fn pop_request(&mut self, ts: u64) -> Option<NodeId> {
        for (node, _addr, state) in &mut self.nodes {
            match state {
                NodeState::Connected { .. } => {
                    *state = NodeState::Requesting { at: ts };
                    return Some(*node);
                }
                _ => {}
            }
        }

        None
    }

    pub fn on_connected_node(&mut self, ts: u64, from_node: NodeId) -> bool {
        for (node, _addr, state) in &mut self.nodes {
            match state {
                NodeState::Connecting { .. } => {
                    if *node == from_node {
                        *state = NodeState::Connected { at: ts };
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    pub fn on_connect_error_node(&mut self, ts: u64, from_node: NodeId) -> bool {
        for (node, _addr, state) in &mut self.nodes {
            match state {
                NodeState::Connecting { .. } => {
                    if *node == from_node {
                        *state = NodeState::ConnectError { at: ts };
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    pub fn on_answered_node(&mut self, ts: u64, from_node: NodeId, res: Vec<(NodeId, NodeAddr, bool)>) -> bool {
        for (node, _addr, state) in &mut self.nodes {
            match state {
                NodeState::Requesting { .. } => {
                    if *node == from_node {
                        *state = NodeState::ReceivedAnswer { at: ts };
                        for (node, addr, connected) in res {
                            self.push_node(ts, node, addr, connected);
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
    use super::{FindKeyRequest, FindKeyRequestStatus};
    use bluesea_identity::{NodeAddr, Protocol};

    #[derive(PartialEq, Debug)]
    enum Msg {}

    #[test]
    fn test_key() {
        let list = FindKeyRequest::new(0, 102, 10000);
        assert_eq!(list.key(), 102);
    }

    #[test]
    fn simple_test_connect() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        list.push_node(0, 2, NodeAddr::from(Protocol::Udp(2)), false);
        list.push_node(0, 3, NodeAddr::from(Protocol::Udp(3)), false);

        assert_eq!(list.pop_connect(0), Some((1, NodeAddr::from(Protocol::Udp(1)))));
        assert_eq!(list.pop_connect(0), Some((2, NodeAddr::from(Protocol::Udp(2)))));
        assert_eq!(list.pop_connect(0), Some((3, NodeAddr::from(Protocol::Udp(3)))));
        assert_eq!(list.pop_connect(0), None);
        assert_eq!(list.pop_request(0), None);
    }

    #[test]
    fn test_unordered_connec() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_node(0, 2, NodeAddr::from(Protocol::Udp(2)), false);
        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        list.push_node(0, 3, NodeAddr::from(Protocol::Udp(3)), false);

        assert_eq!(list.pop_connect(0), Some((1, NodeAddr::from(Protocol::Udp(1)))));
        assert_eq!(list.pop_connect(0), Some((2, NodeAddr::from(Protocol::Udp(2)))));
        assert_eq!(list.pop_connect(0), Some((3, NodeAddr::from(Protocol::Udp(3)))));
        assert_eq!(list.pop_connect(0), None);
        assert_eq!(list.pop_request(0), None);
    }

    #[test]
    fn simple_test_request() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), true);
        list.push_node(0, 2, NodeAddr::from(Protocol::Udp(2)), true);
        list.push_node(0, 3, NodeAddr::from(Protocol::Udp(3)), true);

        assert_eq!(list.pop_request(0), Some(1));
        assert_eq!(list.pop_request(0), Some(2));
        assert_eq!(list.pop_request(0), Some(3));
        assert_eq!(list.pop_request(0), None);
        assert_eq!(list.pop_connect(0), None);
    }

    #[test]
    fn test_unordered_request() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_node(0, 2, NodeAddr::from(Protocol::Udp(2)), true);
        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), true);
        list.push_node(0, 3, NodeAddr::from(Protocol::Udp(3)), true);

        assert_eq!(list.pop_request(0), Some(1));
        assert_eq!(list.pop_request(0), Some(2));
        assert_eq!(list.pop_request(0), Some(3));
        assert_eq!(list.pop_request(0), None);
        assert_eq!(list.pop_connect(0), None);
    }

    #[test]
    fn test_duplicate() {
        let mut list = FindKeyRequest::new(0, 0, 10000);
        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        list.push_node(0, 2, NodeAddr::from(Protocol::Udp(2)), false);
        list.push_node(0, 3, NodeAddr::from(Protocol::Udp(3)), false);

        assert_eq!(list.pop_connect(0), Some((1, NodeAddr::from(Protocol::Udp(1)))));
        assert_eq!(list.pop_connect(0), Some((2, NodeAddr::from(Protocol::Udp(2)))));
        assert_eq!(list.pop_connect(0), Some((3, NodeAddr::from(Protocol::Udp(3)))));
        assert_eq!(list.pop_connect(0), None);
        assert_eq!(list.pop_request(0), None);
    }

    #[test]
    fn test_timeout_not_connect() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.status(10001), FindKeyRequestStatus::Timeout);
    }

    #[test]
    fn test_connect_error() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        list.pop_connect(0);

        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.on_connect_error_node(5000, 2), false);
        assert_eq!(list.on_connect_error_node(5000, 1), true);
        assert_eq!(list.status(10001), FindKeyRequestStatus::Timeout);
    }

    #[test]
    fn test_request_timeout() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        list.pop_connect(0);
        assert_eq!(list.on_connected_node(5000, 2), false);
        assert_eq!(list.on_connected_node(5000, 1), true);
        list.pop_request(0);
        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.status(15001), FindKeyRequestStatus::Timeout);
    }

    #[test]
    fn test_request_success() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_node(0, 1, NodeAddr::from(Protocol::Udp(1)), false);
        list.pop_connect(0);
        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.on_connected_node(5000, 1), true);
        assert_eq!(list.pop_request(0), Some(1));

        assert_eq!(list.status(5000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.on_answered_node(5000, 1, vec![]), true);
        assert_eq!(list.status(15001), FindKeyRequestStatus::Finished);
    }

    #[test]
    fn test_get_better_result() {
        let mut list = FindKeyRequest::new(0, 0, 10000);

        list.push_node(0, 1000, NodeAddr::from(Protocol::Udp(1)), true);
        assert_eq!(list.pop_request(0), Some(1000));
        assert_eq!(list.on_answered_node(1000, 1000, vec![(100, NodeAddr::from(Protocol::Udp(1)), true)]), true);
        assert_eq!(list.status(1000), FindKeyRequestStatus::Requesting);
        assert_eq!(list.pop_request(1000), Some(100));
        assert_eq!(list.status(1000), FindKeyRequestStatus::Requesting);
    }
}

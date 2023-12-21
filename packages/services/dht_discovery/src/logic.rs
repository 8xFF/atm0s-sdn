use crate::find_key_request::{FindKeyRequest, FindKeyRequestStatus};
use crate::kbucket::entry::EntryState;
use crate::kbucket::KBucketTableWrap;
use crate::msg::DiscoveryMsg;
use atm0s_sdn_identity::{NodeAddr, NodeId};
use atm0s_sdn_utils::Timer;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub enum Input {
    AddNode(NodeAddr),
    RefreshKey(NodeId),
    OnTick(u64),
    OnData(NodeId, DiscoveryMsg),
    OnConnected(NodeAddr),
    OnConnectError(NodeId),
    OnDisconnected(NodeId),
}

#[derive(PartialEq, Debug)]
pub enum Action {
    ConnectTo(NodeAddr),
    SendTo(NodeId, DiscoveryMsg),
}

pub struct DiscoveryLogicConf {
    pub local_node_id: NodeId,
    pub timer: Arc<dyn Timer>,
}

pub struct DiscoveryLogic {
    req_id: u32,
    local_node_id: NodeId,
    timer: Arc<dyn Timer>,
    table: KBucketTableWrap,
    action_queues: VecDeque<Action>,
    request_memory: HashMap<u32, FindKeyRequest>,
    refresh_bucket_index: u8,
}

#[allow(dead_code)]
impl DiscoveryLogic {
    pub fn new(conf: DiscoveryLogicConf) -> Self {
        Self {
            req_id: 0,
            local_node_id: conf.local_node_id,
            timer: conf.timer,
            table: KBucketTableWrap::new(conf.local_node_id),
            action_queues: Default::default(),
            request_memory: Default::default(),
            refresh_bucket_index: 0,
        }
    }

    fn check_connected(&self, node: NodeId) -> bool {
        matches!(self.table.get_node(node), Some(EntryState::Connected { .. }))
    }

    fn check_connecting(&self, node: NodeId) -> bool {
        matches!(self.table.get_node(node), Some(EntryState::Connecting { .. }))
    }

    fn process_request(ts: u64, req: &mut FindKeyRequest, table: &mut KBucketTableWrap, action_queues: &mut VecDeque<Action>) {
        while let Some(addr) = req.pop_connect(ts) {
            //Add node to connecting, => 3 case
            // 1. To connecting state => send connect_to
            // 2. Already connecting  => just wait
            // 3. Cannot switch connecting, maybe table full => fire on_connect_error
            if table.add_node_connecting(addr.clone()) {
                action_queues.push_back(Action::ConnectTo(addr));
            } else if table.get_node(addr.node_id()).is_none() {
                req.on_connect_error_node(ts, addr.node_id());
            }
        }

        while let Some(node) = req.pop_request(ts) {
            action_queues.push_back(Action::SendTo(node, DiscoveryMsg::FindKey(req.req_id(), req.key())));
        }
    }

    fn locate_key(&mut self, key: NodeId) {
        let req_id = self.req_id;
        let need_contact_nodes = self.table.closest_nodes(key);
        let now_ms = self.timer.now_ms();
        {
            self.req_id = self.req_id.wrapping_add(1);
            let request = self.request_memory.entry(req_id).or_insert_with(|| FindKeyRequest::new(req_id, key, 30000));

            for (node, addr, connected) in need_contact_nodes {
                request.push_node(now_ms, addr, connected);
            }
            Self::process_request(now_ms, request, &mut self.table, &mut self.action_queues);
        }
    }

    /// add node to table, if it need connect => return true
    fn process_add_node(&mut self, addr: NodeAddr) -> bool {
        if self.table.add_node_connecting(addr.clone()) {
            self.action_queues.push_back(Action::ConnectTo(addr));
            true
        } else {
            false
        }
    }

    pub fn poll_action(&mut self) -> Option<Action> {
        self.action_queues.pop_front()
    }

    pub fn on_input(&mut self, input: Input) {
        match input {
            Input::AddNode(addr) => {
                self.process_add_node(addr);
            }
            Input::RefreshKey(node) => {
                self.locate_key(node);
            }
            Input::OnTick(ts) => {
                let removed_nodes = self.table.remove_timeout_nodes();
                let mut ended_reqs = vec![];
                for removed_node in removed_nodes {
                    for (req_id, req) in &mut self.request_memory {
                        if req.on_connect_error_node(ts, removed_node) && req.is_ended(ts) {
                            ended_reqs.push(*req_id);
                        }
                    }
                }
                for req_id in ended_reqs {
                    self.request_memory.remove(&req_id);
                }

                //If has other request => don't refresh
                if self.table.connected_size() > 0 && self.request_memory.is_empty() {
                    //because of bucket_index from 1 to 32 but refresh_bucket_index from 0 to 31
                    let refresh_index = self.refresh_bucket_index + 1;
                    assert!((1..=32).contains(&refresh_index));
                    let key = u32::MAX >> (32 - refresh_index);
                    self.locate_key(key & self.local_node_id);
                    self.refresh_bucket_index = (self.refresh_bucket_index + 1) % 32;
                }

                let mut timeout_reqs = vec![];
                for (req_id, req) in &self.request_memory {
                    if req.status(ts).is_timeout() {
                        timeout_reqs.push(*req_id);
                    }
                }
                for req_id in timeout_reqs {
                    self.request_memory.remove(&req_id);
                }
            }
            Input::OnData(from_node, data) => match data {
                DiscoveryMsg::FindKey(req_id, key) => {
                    let mut res = vec![];
                    let closest_nodes = self.table.closest_nodes(key);
                    for (node, addr, _connected) in closest_nodes {
                        res.push((node, addr));
                    }
                    self.action_queues.push_back(Action::SendTo(from_node, DiscoveryMsg::FindKeyRes(req_id, res)));
                }
                DiscoveryMsg::FindKeyRes(req_id, nodes) => {
                    let mut res_extended = vec![];
                    for (node, addr) in nodes {
                        res_extended.push((addr, self.check_connected(node)));
                    }
                    if let Some(request) = self.request_memory.get_mut(&req_id) {
                        let now_ms = self.timer.now_ms();
                        if request.on_answered_node(now_ms, from_node, res_extended) {
                            Self::process_request(now_ms, request, &mut self.table, &mut self.action_queues);
                            if request.status(now_ms) == FindKeyRequestStatus::Finished {
                                self.request_memory.remove(&req_id);
                            }
                        }
                    } else {
                    }
                }
            },
            Input::OnConnected(address) => {
                if self.table.add_node_connected(address.clone()) {
                    let now_ms = self.timer.now_ms();
                    for req in self.request_memory.values_mut() {
                        if req.on_connected_node(now_ms, address.node_id()) {
                            Self::process_request(now_ms, req, &mut self.table, &mut self.action_queues);
                        }
                    }
                }
            }
            Input::OnConnectError(node) => {
                if self.table.remove_connecting_node(node) {
                    let now_ms = self.timer.now_ms();
                    let mut ended_reqs = vec![];
                    for (req_id, req) in &mut self.request_memory {
                        if req.on_connect_error_node(now_ms, node) {
                            Self::process_request(now_ms, req, &mut self.table, &mut self.action_queues);
                            if req.is_ended(now_ms) {
                                ended_reqs.push(*req_id);
                            }
                        }
                    }
                    for req_id in ended_reqs {
                        self.request_memory.remove(&req_id);
                    }
                }
            }
            Input::OnDisconnected(node) => if self.table.remove_connected_node(node) {},
        }
    }
}

#[cfg(test)]
mod test {
    use crate::logic::{Action, DiscoveryLogic, DiscoveryLogicConf, DiscoveryMsg, Input};
    use atm0s_sdn_identity::NodeAddr;
    use atm0s_sdn_utils::SystemTimer;
    use std::sync::Arc;

    #[test]
    fn init_bootstrap() {
        let mut logic = DiscoveryLogic::new(DiscoveryLogicConf {
            local_node_id: 0,
            timer: Arc::new(SystemTimer()),
        });

        logic.on_input(Input::AddNode(NodeAddr::empty(1000)));
        logic.on_input(Input::AddNode(NodeAddr::empty(2000)));

        logic.on_input(Input::RefreshKey(0)); //create request 0

        assert_eq!(logic.poll_action(), Some(Action::ConnectTo(NodeAddr::empty(1000))));
        assert_eq!(logic.poll_action(), Some(Action::ConnectTo(NodeAddr::empty(2000))));

        logic.on_input(Input::OnConnected(NodeAddr::empty(2000)));
        logic.on_input(Input::OnConnected(NodeAddr::empty(1000)));

        assert_eq!(logic.poll_action(), Some(Action::SendTo(2000, DiscoveryMsg::FindKey(0, 0))));
        assert_eq!(logic.poll_action(), Some(Action::SendTo(1000, DiscoveryMsg::FindKey(0, 0))));
        assert_eq!(logic.poll_action(), None);
    }

    #[test]
    fn test_disconnect() {
        let mut logic = DiscoveryLogic::new(DiscoveryLogicConf {
            local_node_id: 0,
            timer: Arc::new(SystemTimer()),
        });

        logic.on_input(Input::AddNode(NodeAddr::empty(1000)));
        logic.on_input(Input::AddNode(NodeAddr::empty(2000)));

        assert_eq!(logic.poll_action(), Some(Action::ConnectTo(NodeAddr::empty(1000))));
        assert_eq!(logic.poll_action(), Some(Action::ConnectTo(NodeAddr::empty(2000))));

        logic.on_input(Input::OnConnected(NodeAddr::empty(2000)));
        logic.on_input(Input::OnConnected(NodeAddr::empty(1000)));

        logic.on_input(Input::OnDisconnected(1000));

        logic.on_input(Input::RefreshKey(0)); //create request 0

        assert_eq!(logic.poll_action(), Some(Action::SendTo(2000, DiscoveryMsg::FindKey(0, 0))));
        assert_eq!(logic.poll_action(), None);
    }

    #[test]
    fn test_connect_error() {
        let mut logic = DiscoveryLogic::new(DiscoveryLogicConf {
            local_node_id: 0,
            timer: Arc::new(SystemTimer()),
        });

        logic.on_input(Input::AddNode(NodeAddr::empty(1000)));
        logic.on_input(Input::RefreshKey(0)); //create request 0

        assert_eq!(logic.poll_action(), Some(Action::ConnectTo(NodeAddr::empty(1000))));

        logic.on_input(Input::OnConnectError(1000));

        assert_eq!(logic.request_memory.len(), 0);
        assert_eq!(logic.poll_action(), None);
    }
}

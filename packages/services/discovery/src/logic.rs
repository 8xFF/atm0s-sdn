use crate::find_key_request::{FindKeyRequest, FindKeyRequestStatus};
use crate::kbucket::entry::EntryState;
use crate::kbucket::KBucketTableWrap;
use crate::msg::DiscoveryMsg;
use bluesea_identity::{PeerAddr, PeerId, PeerIdType};
use network::transport::ConnectionSender;
use network::BehaviorAgent;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use utils::Timer;

pub enum Input {
    AddPeer(PeerId, PeerAddr),
    RefreshKey(PeerId),
    OnTick(u64),
    OnData(PeerId, DiscoveryMsg),
    OnConnected(PeerId, PeerAddr),
    OnConnectError(PeerId),
    OnDisconnected(PeerId),
}

#[derive(PartialEq, Debug)]
pub enum Action {
    ConnectTo(PeerId, PeerAddr),
    SendTo(PeerId, DiscoveryMsg),
}

pub struct DiscoveryLogicConf {
    pub local_node_id: PeerId,
    pub timer: Arc<dyn Timer>,
}

pub struct DiscoveryLogic {
    req_id: u32,
    local_node_id: PeerId,
    timer: Arc<dyn Timer>,
    table: KBucketTableWrap,
    action_queues: VecDeque<Action>,
    request_memory: HashMap<u32, FindKeyRequest>,
    refresh_bucket_index: u8,
}

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

    fn check_connected(&self, peer: PeerId) -> bool {
        matches!(
            self.table.get_peer(peer),
            Some(EntryState::Connected { .. })
        )
    }

    fn check_connecting(&self, peer: PeerId) -> bool {
        matches!(
            self.table.get_peer(peer),
            Some(EntryState::Connecting { .. })
        )
    }

    fn process_request(
        ts: u64,
        req: &mut FindKeyRequest,
        table: &mut KBucketTableWrap,
        action_queues: &mut VecDeque<Action>,
    ) {
        while let Some((peer, addr)) = req.pop_connect(ts) {
            //Add peer to connecting, => 3 case
            // 1. To connecting state => send connect_to
            // 2. Already connecting  => just wait
            // 3. Cannot switch connecting, maybe table full => fire on_connect_error
            if table.add_peer_connecting(peer, addr.clone()) {
                action_queues.push_back(Action::ConnectTo(peer, addr));
            } else if table.get_peer(peer).is_none() {
                req.on_connect_error_peer(ts, peer);
            }
        }

        while let Some(peer) = req.pop_request(ts) {
            action_queues.push_back(Action::SendTo(
                peer,
                DiscoveryMsg::FindKey(req.req_id(), req.key()),
            ));
        }
    }

    fn locate_key(&mut self, key: PeerId) {
        let req_id = self.req_id;
        let need_contact_peers = self.table.closest_peers(key);
        let now_ms = self.timer.now_ms();
        {
            self.req_id = self.req_id.wrapping_add(1);
            let request = self
                .request_memory
                .entry(req_id)
                .or_insert_with(|| FindKeyRequest::new(req_id, key, 30000));

            for (peer, addr, connected) in need_contact_peers {
                request.push_peer(now_ms, peer, addr, connected);
            }
            Self::process_request(now_ms, request, &mut self.table, &mut self.action_queues);
        }
    }

    /// add peer to table, if it need connect => return true
    fn process_add_peer(&mut self, peer: PeerId, addr: PeerAddr) -> bool {
        if self.table.add_peer_connecting(peer, addr.clone()) {
            self.action_queues.push_back(Action::ConnectTo(peer, addr));
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
            Input::AddPeer(peer, addr) => {
                self.process_add_peer(peer, addr);
            }
            Input::RefreshKey(peer) => {
                self.locate_key(peer);
            }
            Input::OnTick(ts) => {
                let removed_peers = self.table.remove_timeout_peers();
                let mut ended_reqs = vec![];
                for removed_peer in removed_peers {
                    for (req_id, req) in &mut self.request_memory {
                        if req.on_connect_error_peer(ts, removed_peer) {
                            if req.is_ended(ts) {
                                ended_reqs.push(*req_id);
                            }
                        }
                    }
                }
                for req_id in ended_reqs {
                    self.request_memory.remove(&req_id);
                }

                //If has other request => don't refresh
                if self.table.connected_size() > 0 && self.request_memory.len() == 0 {
                    //because of bucket_index from 1 to 32 but refresh_bucket_index from 0 to 31
                    let refresh_index = self.refresh_bucket_index + 1;
                    assert!(refresh_index >= 1 && refresh_index <= 32);
                    let key = (u32::MAX >> (32 - refresh_index));
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
            Input::OnData(from_peer, data) => match data {
                DiscoveryMsg::FindKey(req_id, key) => {
                    let mut res = vec![];
                    let closest_peers = self.table.closest_peers(key);
                    for (peer, addr, connected) in closest_peers {
                        res.push((peer, addr));
                    }
                    self.action_queues.push_back(Action::SendTo(
                        from_peer,
                        DiscoveryMsg::FindKeyRes(req_id, res),
                    ));
                }
                DiscoveryMsg::FindKeyRes(req_id, peers) => {
                    let mut res_extended = vec![];
                    for (peer, addr) in peers {
                        res_extended.push((peer, addr, self.check_connected(peer)));
                    }
                    if let Some(request) = self.request_memory.get_mut(&req_id) {
                        let now_ms = self.timer.now_ms();
                        if request.on_answered_peer(now_ms, from_peer, res_extended) {
                            Self::process_request(
                                now_ms,
                                request,
                                &mut self.table,
                                &mut self.action_queues,
                            );
                            if request.status(now_ms) == FindKeyRequestStatus::Finished {
                                self.request_memory.remove(&req_id);
                            }
                        }
                    } else {
                    }
                }
            },
            Input::OnConnected(peer, address) => {
                if self.table.add_peer_connected(peer, address) {
                    let now_ms = self.timer.now_ms();
                    for (req_id, req) in &mut self.request_memory {
                        if req.on_connected_peer(now_ms, peer) {
                            Self::process_request(
                                now_ms,
                                req,
                                &mut self.table,
                                &mut self.action_queues,
                            );
                        }
                    }
                }
            }
            Input::OnConnectError(peer) => {
                if self.table.remove_connecting_peer(peer) {
                    let now_ms = self.timer.now_ms();
                    let mut ended_reqs = vec![];
                    for (req_id, req) in &mut self.request_memory {
                        if req.on_connect_error_peer(now_ms, peer) {
                            Self::process_request(
                                now_ms,
                                req,
                                &mut self.table,
                                &mut self.action_queues,
                            );
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
            Input::OnDisconnected(peer) => if self.table.remove_connected_peer(peer) {},
        }
    }
}

#[cfg(test)]
mod test {
    use crate::logic::{Action, DiscoveryLogic, DiscoveryLogicConf, DiscoveryMsg, Input};
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::PeerAddr;
    use std::sync::Arc;
    use utils::SystemTimer;

    #[test]
    fn init_bootstrap() {
        let mut logic = DiscoveryLogic::new(DiscoveryLogicConf {
            local_node_id: 0,
            timer: Arc::new(SystemTimer()),
        });

        logic.on_input(Input::AddPeer(1000, PeerAddr::from(Protocol::Udp(1000))));
        logic.on_input(Input::AddPeer(2000, PeerAddr::from(Protocol::Udp(2000))));

        logic.on_input(Input::RefreshKey(0)); //create request 0

        assert_eq!(
            logic.poll_action(),
            Some(Action::ConnectTo(1000, PeerAddr::from(Protocol::Udp(1000))))
        );
        assert_eq!(
            logic.poll_action(),
            Some(Action::ConnectTo(2000, PeerAddr::from(Protocol::Udp(2000))))
        );

        logic.on_input(Input::OnConnected(
            2000,
            PeerAddr::from(Protocol::Udp(2000)),
        ));
        logic.on_input(Input::OnConnected(
            1000,
            PeerAddr::from(Protocol::Udp(1000)),
        ));

        assert_eq!(
            logic.poll_action(),
            Some(Action::SendTo(2000, DiscoveryMsg::FindKey(0, 0)))
        );
        assert_eq!(
            logic.poll_action(),
            Some(Action::SendTo(1000, DiscoveryMsg::FindKey(0, 0)))
        );
        assert_eq!(logic.poll_action(), None);
    }

    #[test]
    fn test_disconnect() {
        let mut logic = DiscoveryLogic::new(DiscoveryLogicConf {
            local_node_id: 0,
            timer: Arc::new(SystemTimer()),
        });

        logic.on_input(Input::AddPeer(1000, PeerAddr::from(Protocol::Udp(1000))));
        logic.on_input(Input::AddPeer(2000, PeerAddr::from(Protocol::Udp(2000))));

        assert_eq!(
            logic.poll_action(),
            Some(Action::ConnectTo(1000, PeerAddr::from(Protocol::Udp(1000))))
        );
        assert_eq!(
            logic.poll_action(),
            Some(Action::ConnectTo(2000, PeerAddr::from(Protocol::Udp(2000))))
        );

        logic.on_input(Input::OnConnected(
            2000,
            PeerAddr::from(Protocol::Udp(2000)),
        ));
        logic.on_input(Input::OnConnected(
            1000,
            PeerAddr::from(Protocol::Udp(1000)),
        ));

        logic.on_input(Input::OnDisconnected(1000));

        logic.on_input(Input::RefreshKey(0)); //create request 0

        assert_eq!(
            logic.poll_action(),
            Some(Action::SendTo(2000, DiscoveryMsg::FindKey(0, 0)))
        );
        assert_eq!(logic.poll_action(), None);
    }

    #[test]
    fn test_connect_error() {
        let mut logic = DiscoveryLogic::new(DiscoveryLogicConf {
            local_node_id: 0,
            timer: Arc::new(SystemTimer()),
        });

        logic.on_input(Input::AddPeer(1000, PeerAddr::from(Protocol::Udp(1000))));
        logic.on_input(Input::RefreshKey(0)); //create request 0

        assert_eq!(
            logic.poll_action(),
            Some(Action::ConnectTo(1000, PeerAddr::from(Protocol::Udp(1000))))
        );

        logic.on_input(Input::OnConnectError(1000));

        assert_eq!(logic.request_memory.len(), 0);
        assert_eq!(logic.poll_action(), None);
    }
}

use crate::closest_list::ClosestList;
use crate::kbucket::entry::EntryState;
use crate::kbucket::KBucketTableWrap;
use bluesea_identity::{PeerAddr, PeerId, PeerIdType};
use network::plane::BehaviorAgent;
use network::transport::ConnectionSender;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use utils::Timer;
use crate::msg::DiscoveryMsg;

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
    request_memory: HashMap<u32, ClosestList<DiscoveryMsg>>,
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
            refresh_bucket_index: 0
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

    fn locate_key(&mut self, key: PeerId) {
        let req_id = self.req_id;
        let need_contact_peers = self.table.closest_peers(key);
        let local_peer_id = self.local_node_id;
        let now_ms = self.timer.now_ms();
        {
            self.req_id = self.req_id.wrapping_add(1);
            let request = self
                .request_memory
                .entry(req_id)
                .or_insert_with(|| ClosestList::new(key, local_peer_id, now_ms));

            for (peer, addr, connected) in need_contact_peers {
                request.add_peer(peer, addr, connected);
                if connected {
                    self.action_queues
                        .push_back(Action::SendTo(peer, DiscoveryMsg::FindKey(req_id, key)));
                } else {
                    request.add_pending_msg(peer, DiscoveryMsg::FindKey(req_id, key));
                }
            }
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
            Input::OnTick(_) => {
                let removed_peers = self.table.remove_timeout_peers();
                for removed_peer in removed_peers {
                    for (_req_id, req) in &mut self.request_memory {
                        while let Some(_) = req.pop_pending_msg(removed_peer) {}
                    }
                }

                if self.table.connected_size() > 0 {
                    //because of bucket_index from 1 to 32 but refresh_bucket_index from 0 to 31
                    let refresh_index = self.refresh_bucket_index + 1;
                    assert!(refresh_index >=1 && refresh_index <= 32);
                    let key = (u32::MAX >> (32 - refresh_index));
                    self.locate_key(key & self.local_node_id );
                    self.refresh_bucket_index = (self.refresh_bucket_index + 1) % 32;
                }
            }
            Input::OnData(from_peer, data) => match data {
                DiscoveryMsg::FindKey(req_id, key) => {
                    let mut res = vec![];
                    let closest_peers = self.table.closest_peers(key);
                    for (peer, addr, connected) in closest_peers {
                        res.push((peer, addr));
                    }
                    self.action_queues
                        .push_back(Action::SendTo(from_peer, DiscoveryMsg::FindKeyRes(req_id, res)));
                }
                DiscoveryMsg::FindKeyRes(req_id, peers) => {
                    let mut key = None;
                    let mut need_contact_list = vec![];
                    {
                        if let Some(request) = self.request_memory.get_mut(&req_id) {
                            key = Some(request.get_key());
                            for (peer, addr) in &peers {
                                request.add_peer(*peer, addr.clone(), false);
                            }
                            while let Some(dest) = request.pop_need_connect() {
                                need_contact_list.push(dest);
                            }
                        }
                    }
                    if let Some(key) = key {
                        for (peer, addr) in need_contact_list {
                            if self.check_connected(peer) {
                                self.action_queues
                                    .push_back(Action::SendTo(peer, DiscoveryMsg::FindKey(req_id, key)));
                            } else if self.check_connecting(peer) {
                                if let Some(request) = self.request_memory.get_mut(&req_id) {
                                    request.add_pending_msg(peer, DiscoveryMsg::FindKey(req_id, key));
                                }
                            } else {
                                self.action_queues.push_back(Action::ConnectTo(peer, addr));
                            }
                        }
                    }
                }
            },
            Input::OnConnected(peer, address) => {
                if self.table.add_peer_connected(peer, address) {
                    for (_req_id, req) in &mut self.request_memory {
                        while let Some(msg) = req.pop_pending_msg(peer) {
                            self.action_queues.push_back(Action::SendTo(peer, msg));
                        }
                    }
                }
            }
            Input::OnConnectError(peer) => {
                if self.table.remove_connecting_peer(peer) {
                    for (_req_id, req) in &mut self.request_memory {
                        while let Some(_) = req.pop_pending_msg(peer) {}
                    }
                }
            }
            Input::OnDisconnected(peer) => if self.table.remove_connected_peer(peer) {},
        }
    }
}

#[cfg(test)]
mod test {
    use crate::logic::{Action, DiscoveryLogic, DiscoveryLogicConf, Input, DiscoveryMsg};
    use std::sync::Arc;
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::PeerAddr;
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

        logic.on_input(Input::OnConnected(2000, PeerAddr::from(Protocol::Udp(2000))));
        logic.on_input(Input::OnConnected(1000, PeerAddr::from(Protocol::Udp(1000))));

        assert_eq!(
            logic.poll_action(),
            Some(Action::SendTo(2000, DiscoveryMsg::FindKey(0, 0)))
        );
        assert_eq!(
            logic.poll_action(),
            Some(Action::SendTo(1000, DiscoveryMsg::FindKey(0, 0)))
        );
    }
}

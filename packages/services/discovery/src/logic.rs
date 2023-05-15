use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use bluesea_identity::{PeerAddr, PeerId, PeerIdType};
use kademlia::kbucket::{Entry, NodeStatus, KBucketsTable, AppliedPending};
use kademlia::kbucket::key::Key;
use network::plane::NetworkAgent;
use network::transport::ConnectionSender;
use utils::Timer;
use crate::closest_list::ClosestList;

pub enum Message {
    FindKey(u32, PeerId),
    FindKeyRes(u32, Vec<(PeerId, PeerAddr)>),
}

pub enum Input {
    AddPeer(PeerId, PeerAddr),
    RefreshKey(PeerId),
    OnTick(u64),
    OnData(PeerId, Message),
    OnConnected(PeerId, PeerAddr),
    OnConnectError(PeerId),
    OnDisconnected(PeerId),
}

pub enum Action {
    ConnectTo(PeerId, PeerAddr),
    SendTo(PeerId, Message)
}

pub struct DiscoveryLogicConf {
    pub local_node_id: PeerId,
    pub timer: Arc<dyn Timer>,
}

pub struct DiscoveryLogic {
    req_id: u32,
    local_node_id: PeerId,
    local_key: Key<PeerId>,
    timer: Arc<dyn Timer>,
    table: KBucketsTable<Key<PeerId>, PeerAddr>,
    connecting_peers: HashMap<PeerId, u64>,
    action_queues: VecDeque<Action>,
    request_memory: HashMap<u32, ClosestList<Message>>,
}

impl DiscoveryLogic {
    pub fn new(conf: DiscoveryLogicConf) -> Self {
        let local_key: Key<PeerId> = conf.local_node_id.into();
        Self {
            req_id: 0,
            local_node_id: conf.local_node_id,
            local_key: local_key.clone(),
            timer: conf.timer,
            connecting_peers: Default::default(),
            table: KBucketsTable::new(local_key, Duration::from_secs(30)),
            action_queues: Default::default(),
            request_memory: Default::default(),
        }
    }

    fn is_connected(&self, peer: PeerId) -> bool {
        todo!()
        // if let Some(bucket) = self.table.bucket::<Key<PeerId>>(&peer.into()) {
        //     if bucket.contains(&self.local_key.distance::<Key<PeerId>>(&peer.into())) {
        //         return true;
        //     }
        // }
        // false
    }

    fn locate_value(&mut self, value: PeerId) {
        let req_id = self.req_id;
        self.req_id = self.req_id.wrapping_add(1);
        let request = self.request_memory.entry(req_id)
            .or_insert_with(|| Default::default());

        let key = value.into();
        let iter = self.table.closest::<Key<PeerId>>(&key);
        for entry in iter {
            request.add_peer(*entry.node.key.preimage(), entry.node.value, true);
        }

        while let Some((peer, addr)) = request.pop_need_connect() {
            todo!()
            // if self.is_connected(peer) {
            //     self.action_queues.push_back(Action::SendTo(peer, Message::FindKey(req_id, value)));
            // } else {
            //     self.action_queues.push_back(Action::ConnectTo(peer, addr));
            //     request.add_pending_msg(peer, Message::FindKey(req_id, value));
            // }
        }
    }

    /// add peer to table, if it already connected => return true
    fn process_add_peer(&mut self, peer: PeerId, addr: PeerAddr) -> bool {
        if !self.connecting_peers.contains_key(&peer) {
            if let Some(bucket) = self.table.bucket::<Key<PeerId>>(&peer.into()) {
                if bucket.contains(&self.local_key.distance::<Key<PeerId>>(&peer.into())) {
                    //Already has connection => don't need connect
                    true
                } else {
                    self.connecting_peers.insert(peer, self.timer.now_ms());
                    self.action_queues.push_back(Action::ConnectTo(peer, addr));
                    false
                }
            } else {
                self.connecting_peers.insert(peer, self.timer.now_ms());
                self.action_queues.push_back(Action::ConnectTo(peer, addr));
                false
            }
        } else {
            true
        }
    }

    pub fn poll_action(&mut self) -> Option<Action> {
        self.action_queues.pop_front()
    }

    pub fn on_input(&mut self, input: Input) {
        match input {
            Input::AddPeer(peer, addr) => {
                if !self.process_add_peer(peer, addr) {
                    self.locate_value(self.local_node_id);
                }
            }
            Input::RefreshKey(peer) => {
                self.locate_value(peer);
            }
            Input::OnTick(_) => {
                let iter = self.table.iter();
                //loop for apply pending
                for _ in iter {}
                while let Some(pending) = self.table.take_applied_pending() {
                    match pending {
                        AppliedPending { inserted, evicted } => {
                            //TODO
                        }
                    }
                }
            }
            Input::OnData(from_peer, data) => {
                match data {
                    Message::FindKey(req_id, peer) => {
                        let mut res = vec![];
                        let key = peer.into();
                        let iter = self.table.closest::<Key<PeerId>>(&key);
                        for entry in iter {
                            res.push((*entry.node.key.preimage(), entry.node.value));
                        }
                        self.action_queues.push_back(Action::SendTo(from_peer, Message::FindKeyRes(req_id, res)));
                    }
                    Message::FindKeyRes(req_id, peers) => {
                        for (peer, addr) in peers {
                            self.process_add_peer(peer, addr);
                        }
                    }
                }
            }
            Input::OnConnected(peer, address) => {
                self.connecting_peers.remove(&peer);
                let key = peer.into();
                let mut entry = self.table.entry(&key);
                match entry {
                    Entry::Present(mut entry, _) => {
                        *entry.value() = address;
                        entry.update(NodeStatus::Connected);
                    }
                    Entry::Pending(mut entry, _) => {
                        *entry.value() = address;
                        entry.update(NodeStatus::Connected);
                    }
                    Entry::Absent(mut entry) => {
                        entry.insert(address, NodeStatus::Connected);
                    }
                    Entry::SelfEntry => {}
                }
            }
            Input::OnConnectError(peer) => {
                self.connecting_peers.remove(&peer);
            }
            Input::OnDisconnected(peer) => {
                match self.table.entry(&peer.into()) {
                    Entry::Present(mut entry, _) => {
                        entry.update(NodeStatus::Disconnected);
                    }
                    Entry::Pending(mut entry, _) => {
                        entry.update(NodeStatus::Disconnected);
                    }
                    Entry::Absent(_) => {}
                    Entry::SelfEntry => {}
                }
            }
        }
    }
}
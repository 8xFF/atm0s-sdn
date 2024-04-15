//! Source Hint is a way we create a notification tree for sources and subscribers.
//! Because if this method is not latency focus thefore it only used for low frequency message like Source Changed notification.
//!
//! The main idea is instead of a single node take care of all subscribers, which can cause overload and waste of resource,
//! we create a tree of nodes that relay the message to the next hop.
//!
//! Term: root node is the node which closest to channel id in XOR distance.
//!
//! Subscribe: a node need to subscribe a channel notification, it will send a message to next node.
//! Each time a node receive a subscribe message it will add the subscriber to the list and send a message to next hop.
//!
//! Register: a node need to register as a source for channel, it will send a message to next node.
//! Each time a node receive a register message it will add the source to the list and send:
//!     - to all subscribers except sender if it is a new source.
//!     - to next hop
//!
//! For ensure keep the tree clean and sync, each Subscriber and Source will resend message in each 1 seconds for keep alive.
//! For solve the case which network chaged cause root node changed, a node receive Register or Subscribe will reply with a RegisterOk, SubscribeOk
//! for that, the sender will know the real next hop and only accept notification from selected node.

use std::{
    collections::{BTreeMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::NodeId;

use crate::{base::FeatureControlActor, features::pubsub::msg::SourceHint};

const TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCmd {
    Register,
    Unregister,
    Subscribe,
    Unsubscribe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    SendRemote(Option<SocketAddr>, SourceHint),
    SubscribeSource(Vec<FeatureControlActor>, NodeId),
    UnsubscribeSource(Vec<FeatureControlActor>, NodeId),
}

#[derive(Default)]
pub struct SourceHintLogic {
    node_id: NodeId,
    session_id: u64,
    remote_sources: BTreeMap<NodeId, u64>,
    remote_subscribers: BTreeMap<SocketAddr, u64>,
    local_sources: Vec<FeatureControlActor>,
    local_subscribers: Vec<FeatureControlActor>,
    next_hop: Option<SocketAddr>,
    queue: VecDeque<Output>,
}

impl SourceHintLogic {
    pub fn new(node_id: NodeId, session_id: u64) -> Self {
        Self {
            node_id,
            session_id,
            ..Default::default()
        }
    }

    /// return sources as local and remote sources
    pub fn sources(&self) -> Vec<NodeId> {
        if self.local_sources.is_empty() {
            self.remote_sources.keys().cloned().collect()
        } else {
            self.remote_sources.keys().chain(std::iter::once(&self.node_id)).cloned().collect()
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        let mut timeout_subscribes = vec![];
        for (remote, last_tick) in &self.remote_subscribers {
            if now_ms - last_tick >= TIMEOUT_MS {
                timeout_subscribes.push(*remote);
            }
        }
        for subscriber in timeout_subscribes {
            self.remote_subscribers.remove(&subscriber);
            // if all subscribers are removed, we need to notify next hop to remove this node from subscriber list
            if self.local_subscribers.is_empty() && self.remote_subscribers.is_empty() {
                log::warn!("[SourceHint] Send Unsubscribe({}) to next node because remote subscriber {subscriber} timeout", self.session_id);
                self.queue.push_back(Output::SendRemote(None, SourceHint::Unsubscribe(self.session_id)));
            }
        }

        let mut timeout_sources = vec![];
        for (source, last_tick) in &self.remote_sources {
            if now_ms - last_tick >= TIMEOUT_MS {
                timeout_sources.push(*source);
            }
        }
        for source in timeout_sources {
            self.remote_sources.remove(&source);
            if !self.local_subscribers.is_empty() {
                log::warn!("[SourceHint] Notify remove source({source}) to local {:?} actors because timeout", self.local_subscribers);
                self.queue.push_back(Output::UnsubscribeSource(self.local_subscribers.clone(), source));
            }
        }

        if !self.local_sources.is_empty() {
            log::debug!("[SourceHint] ReSend Register({}) to root node", self.node_id);
            self.queue.push_back(Output::SendRemote(None, SourceHint::Register { source: self.node_id, to_root: true }));
            for (remote, _) in &self.remote_subscribers {
                log::debug!("[SourceHint] ReSend Register({}) to subscribe {remote} node", self.node_id);
                self.queue.push_back(Output::SendRemote(Some(*remote), SourceHint::Register { source: self.node_id, to_root: false }));
            }
        }

        if !self.local_subscribers.is_empty() || !self.remote_subscribers.is_empty() {
            log::debug!("[SourceHint] ReSend Subscribe({}) to next node", self.session_id);
            self.queue.push_back(Output::SendRemote(None, SourceHint::Subscribe(self.session_id)));
        }
    }

    pub fn on_local(&mut self, _now_ms: u64, actor: FeatureControlActor, cmd: LocalCmd) {
        match cmd {
            LocalCmd::Register => {
                if !self.local_sources.contains(&actor) {
                    log::info!("[SourceHint] Register new local source: {:?}", actor);
                    self.local_sources.push(actor);
                    if self.local_sources.len() == 1 && self.remote_sources.is_empty() {
                        log::info!("[SourceHint] Send Register({}) to root node", self.node_id);
                        self.queue.push_back(Output::SendRemote(None, SourceHint::Register { source: self.node_id, to_root: true }));
                    }

                    if self.local_sources.len() == 1 {
                        for (remote, _) in &self.remote_subscribers {
                            log::info!("[SourceHint] Notify Register({}) to subscribe {remote} node", self.node_id);
                            self.queue.push_back(Output::SendRemote(Some(*remote), SourceHint::Register { source: self.node_id, to_root: false }));
                        }
                        if !self.local_subscribers.is_empty() {
                            log::info!("[SourceHint] Notify new source({}) to local {:?} actors", self.node_id, self.local_subscribers);
                            self.queue.push_back(Output::SubscribeSource(self.local_subscribers.clone(), self.node_id));
                        }
                    }
                }
            }
            LocalCmd::Unregister => {
                if let Some(index) = self.local_sources.iter().position(|x| x == &actor) {
                    log::info!("[SourceHint] Unregister local source: {:?}", actor);
                    self.local_sources.swap_remove(index);
                    if self.local_sources.is_empty() && self.remote_sources.is_empty() {
                        log::info!("[SourceHint] Send Unregister({}) to root node", self.node_id);
                        self.queue.push_back(Output::SendRemote(None, SourceHint::Unregister { source: self.node_id, to_root: true }));
                    }

                    if self.local_sources.is_empty() {
                        for (remote, _) in &self.remote_subscribers {
                            log::info!("[SourceHint] Notify Unregister({}) to subscribe {remote} node", self.node_id);
                            self.queue.push_back(Output::SendRemote(Some(*remote), SourceHint::Unregister { source: self.node_id, to_root: false }));
                        }
                        if !self.local_subscribers.is_empty() {
                            log::info!("[SourceHint] Notify removed source({}) to local {:?} actors", self.node_id, self.local_subscribers);
                            self.queue.push_back(Output::UnsubscribeSource(self.local_subscribers.clone(), self.node_id));
                        }
                    }
                }
            }
            LocalCmd::Subscribe => {
                if !self.local_subscribers.contains(&actor) {
                    log::info!("[SourceHint] Subscribe new local subscriber: {:?}", actor);
                    self.local_subscribers.push(actor);
                    if self.local_subscribers.len() == 1 && self.remote_subscribers.is_empty() {
                        log::info!("[SourceHint] Send Subscribe({}) to root node", self.session_id);
                        self.queue.push_back(Output::SendRemote(None, SourceHint::Subscribe(self.session_id)));
                    }

                    for source in self.remote_sources.keys() {
                        log::info!("[SourceHint] Notify SubscribeSource for already added sources({source}) to actor {:?}", actor);
                        self.queue.push_back(Output::SubscribeSource(vec![actor], *source));
                    }
                    if !self.local_sources.is_empty() {
                        self.queue.push_back(Output::SubscribeSource(vec![actor], self.node_id));
                    }
                }
            }
            LocalCmd::Unsubscribe => {
                if let Some(index) = self.local_subscribers.iter().position(|x| x == &actor) {
                    log::info!("[SourceHint] Unsubscribe local subscriber: {:?}", actor);
                    self.local_subscribers.swap_remove(index);
                    // if all subscribers are removed, we need to notify next hop to remove this node from subscriber list
                    if self.local_subscribers.is_empty() && self.remote_subscribers.is_empty() {
                        log::info!("[SourceHint] Send Unsubscribe({}) to next node", self.session_id);
                        self.queue.push_back(Output::SendRemote(None, SourceHint::Unsubscribe(self.session_id)));
                    }

                    // we unsubscriber for the local actor from all remote sources
                    for source in self.remote_sources.keys() {
                        log::info!("[SourceHint] Notify UnsubscribeSource for already added sources({source}) to actor {:?}", actor);
                        self.queue.push_back(Output::UnsubscribeSource(vec![actor], *source));
                    }
                    // we unsubscriber for the local actor from local source
                    if !self.local_sources.is_empty() {
                        self.queue.push_back(Output::UnsubscribeSource(vec![actor], self.node_id));
                    }
                }
            }
        }
    }

    pub fn on_remote(&mut self, now_ms: u64, remote: SocketAddr, cmd: SourceHint) {
        match cmd {
            SourceHint::Register { source, to_root } => {
                // We only accept register from original source and go up to root node, or notify from next_hop (parent node1)
                if !to_root && self.next_hop != Some(remote) {
                    log::warn!("[SourceHint] remote Register({source}) relayed from {remote} is not from next hop, ignore it");
                    return;
                }

                // Only to_root msg is climb up to root node
                if to_root {
                    log::debug!("[SourceHint] forward remote Register({}) to root node", source);
                    self.queue.push_back(Output::SendRemote(None, SourceHint::Register { source, to_root: true }));
                }

                for (subscriber, _) in &self.remote_subscribers {
                    if subscriber.eq(&remote) {
                        // for avoiding loop, we only send to other subscribers except sender
                        continue;
                    }
                    log::debug!("[SourceHint] forward remote Register({}) to subscribe {subscriber} node", source);
                    self.queue.push_back(Output::SendRemote(Some(*subscriber), SourceHint::Register { source, to_root: false }));
                }

                // if source is new, notify to all local subscribers
                if self.remote_sources.insert(source, now_ms).is_none() {
                    log::info!("[SourceHint] added remote source {source}");
                    if !self.local_subscribers.is_empty() {
                        log::info!("[SourceHint] Notify new source({}) to local {:?} actors", source, self.local_subscribers);
                        self.queue.push_back(Output::SubscribeSource(self.local_subscribers.clone(), source));
                    }
                }
            }
            SourceHint::Unregister { source, to_root } => {
                // We only accept unregister from original source and go up to root node, or notify from next_hop (parent node1
                if !to_root && self.next_hop != Some(remote) {
                    log::warn!("[SourceHint] Unregister({source}) relayed from {remote} is not from next hop, ignore it");
                    return;
                }

                // Only to_root msg is climb up to root node
                if to_root {
                    log::debug!("[SourceHint] Send Register({}) to root node", self.node_id);
                    self.queue.push_back(Output::SendRemote(None, SourceHint::Unregister { source, to_root: true }));
                }

                for (subscriber, _) in &self.remote_subscribers {
                    if subscriber.eq(&remote) {
                        // for avoiding loop, we only send to other subscribers except sender
                        continue;
                    }
                    log::debug!("[SourceHint] relay UnRegister({}) to subscribe {subscriber} node", source);
                    self.queue.push_back(Output::SendRemote(Some(*subscriber), SourceHint::Unregister { source, to_root: false }));
                }

                // if source is deleted, notify to all local subscribers
                if self.remote_sources.remove(&source).is_some() {
                    log::info!("[SourceHint] removed remote source {source}");
                    if !self.local_subscribers.is_empty() {
                        log::info!("[SourceHint] Notify remove source({}) to local {:?} actors", self.node_id, self.local_subscribers);
                        self.queue.push_back(Output::UnsubscribeSource(self.local_subscribers.clone(), source));
                    }
                }
            }
            SourceHint::Subscribe(session) => {
                if self.remote_subscribers.insert(remote, now_ms).is_none() {
                    log::info!("[SourceHint] added remote subscriber {remote}");
                    self.queue.push_back(Output::SendRemote(Some(remote), SourceHint::SubscribeOk(session)));
                    if self.remote_subscribers.len() == 1 && self.local_subscribers.is_empty() {
                        log::info!("[SourceHint] Send Subscribe({}) to root node", self.session_id);
                        self.queue.push_back(Output::SendRemote(None, SourceHint::Subscribe(self.session_id)));
                    }

                    let mut sources = self.remote_sources.keys().cloned().collect::<Vec<_>>();
                    if !self.local_sources.is_empty() {
                        sources.push(self.node_id);
                    }

                    if !sources.is_empty() {
                        log::info!("[SourceHint] Notify SubscribeSource for already added sources({:?}) to remote {remote}", sources);
                        self.queue.push_back(Output::SendRemote(Some(remote), SourceHint::Sources(sources)));
                    }
                } else {
                    //TODO check session for resend ACK
                    self.queue.push_back(Output::SendRemote(Some(remote), SourceHint::SubscribeOk(session)));
                }
            }
            SourceHint::SubscribeOk(session) => {
                if session == self.session_id {
                    self.next_hop = Some(remote);
                }
            }
            SourceHint::Unsubscribe(session) => {
                if self.remote_subscribers.remove(&remote).is_some() {
                    log::info!("[SourceHint] removed remote subscriber {remote}");
                    self.queue.push_back(Output::SendRemote(Some(remote), SourceHint::UnsubscribeOk(session)));
                    // if all subscribers are removed, we need to notify next hop to remove this node from subscriber list
                    if self.local_subscribers.is_empty() && self.remote_subscribers.is_empty() {
                        log::info!("[SourceHint] Send Unsubscribe({}) to next node because all subscribers removed", self.session_id);
                        self.queue.push_back(Output::SendRemote(None, SourceHint::Unsubscribe(self.session_id)));
                    }
                } else {
                    //check session for resend ACK
                    self.queue.push_back(Output::SendRemote(Some(remote), SourceHint::UnsubscribeOk(session)));
                }
            }
            SourceHint::UnsubscribeOk(session) => {
                if session == self.session_id {
                    self.next_hop = None;
                }
            }
            SourceHint::Sources(sources) => {
                for source in sources {
                    if self.remote_sources.insert(source, now_ms).is_none() {
                        log::info!("[SourceHint] added remote source {source}");
                        for remote in self.remote_subscribers.keys() {
                            log::debug!("[SourceHint] Notify source({source}) from snapshot to remote {remote}");
                            self.queue.push_back(Output::SendRemote(Some(*remote), SourceHint::Register { source, to_root: false }));
                        }
                        if !self.local_subscribers.is_empty() {
                            log::info!("[SourceHint] Notify new source({source}) to local {:?} actors", self.local_subscribers);
                            self.queue.push_back(Output::SubscribeSource(self.local_subscribers.clone(), source));
                        }
                    }
                }
            }
        }
    }

    pub fn pop_output(&mut self) -> Option<Output> {
        self.queue.pop_front()
    }

    pub fn should_clear(&self) -> bool {
        self.local_sources.is_empty() && self.remote_sources.is_empty() && self.local_subscribers.is_empty() && self.remote_subscribers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, vec};

    use crate::{base::FeatureControlActor, features::pubsub::controller::source_hint::TIMEOUT_MS};

    use super::{LocalCmd, Output, SourceHint, SourceHintLogic};

    #[test]
    fn local_subscribe_should_send_event() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        //subscribe should send a subscribe message
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);
        assert_eq!(sh.pop_output(), None);

        sh.on_local(0, FeatureControlActor::Worker(1), LocalCmd::Subscribe);
        assert_eq!(sh.pop_output(), None);

        //fake a local source should send local source event
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Register);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Register { source: node_id, to_root: true })));
        assert_eq!(
            sh.pop_output(),
            Some(Output::SubscribeSource(vec![FeatureControlActor::Controller, FeatureControlActor::Worker(1)], node_id))
        );
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn local_subscribe_should_handle_sources() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        //subscribe should send a subscribe message
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);

        sh.on_remote(100, remote, SourceHint::Sources(vec![2, 3]));
        assert_eq!(sh.pop_output(), Some(Output::SubscribeSource(vec![FeatureControlActor::Controller], 2)));
        assert_eq!(sh.pop_output(), Some(Output::SubscribeSource(vec![FeatureControlActor::Controller], 3)));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn local_register_should_send_event() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Register);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Register { source: node_id, to_root: true })));
        assert_eq!(sh.pop_output(), None);

        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Register);
        assert_eq!(sh.pop_output(), None);

        sh.on_local(0, FeatureControlActor::Worker(1), LocalCmd::Register);
        assert_eq!(sh.pop_output(), None);

        //subscribe should send a subscribe message and local source event
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SubscribeSource(vec![FeatureControlActor::Controller], node_id)));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn remote_subscribe_should_send_event() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote_session_id = 4321;

        //fake a local source
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Register);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Register { source: node_id, to_root: true })));

        //remote subscribe should receive a subscribe ok and snapshot of sources
        sh.on_remote(0, remote, SourceHint::Subscribe(remote_session_id));

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote), SourceHint::SubscribeOk(remote_session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote), SourceHint::Sources(vec![node_id]))));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn remote_register_should_send_event() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote_node_id = 2;

        //remote register should send event
        sh.on_remote(
            0,
            remote,
            SourceHint::Register {
                source: remote_node_id,
                to_root: true,
            },
        );

        assert_eq!(
            sh.pop_output(),
            Some(Output::SendRemote(
                None,
                SourceHint::Register {
                    source: remote_node_id,
                    to_root: true
                }
            ))
        );
        assert_eq!(sh.pop_output(), None);

        //subscribe should send a subscribe message and local source event
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SubscribeSource(vec![FeatureControlActor::Controller], remote_node_id)));
        assert_eq!(sh.pop_output(), None);

        //unsubscribe should send a unsubscribe message and local source event
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Unsubscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Unsubscribe(session_id))));
        assert_eq!(sh.pop_output(), Some(Output::UnsubscribeSource(vec![FeatureControlActor::Controller], remote_node_id)));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn remote_notify_register_should_not_climb_to_root() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote_node_id = 2;

        //remote register should send event
        sh.on_remote(
            0,
            remote,
            SourceHint::Register {
                source: remote_node_id,
                to_root: false,
            },
        );
        assert_eq!(sh.pop_output(), None);

        sh.on_remote(
            0,
            remote,
            SourceHint::Unregister {
                source: remote_node_id,
                to_root: false,
            },
        );
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn remote_register_should_not_resend_same_sender() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        let remote1 = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote1_node_id = 2;
        let remote1_session_id = 4321;

        let remote2 = SocketAddr::new([127, 0, 0, 2].into(), 1234);
        let remote2_session_id = 4322;

        //remote subscribe should receive a subscribe ok and snapshot of sources
        sh.on_remote(0, remote1, SourceHint::Subscribe(remote1_session_id));
        sh.on_remote(0, remote2, SourceHint::Subscribe(remote2_session_id));

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote1), SourceHint::SubscribeOk(remote1_session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote2), SourceHint::SubscribeOk(remote2_session_id))));
        assert_eq!(sh.pop_output(), None);

        //remote register should send event except sender
        sh.on_remote(
            0,
            remote1,
            SourceHint::Register {
                source: remote1_node_id,
                to_root: true,
            },
        );

        assert_eq!(
            sh.pop_output(),
            Some(Output::SendRemote(
                None,
                SourceHint::Register {
                    source: remote1_node_id,
                    to_root: true
                }
            ))
        );
        assert_eq!(
            sh.pop_output(),
            Some(Output::SendRemote(
                Some(remote2),
                SourceHint::Register {
                    source: remote1_node_id,
                    to_root: false
                }
            ))
        );
        assert_eq!(sh.pop_output(), None);

        //remote register should send event except sender
        sh.on_remote(
            0,
            remote1,
            SourceHint::Unregister {
                source: remote1_node_id,
                to_root: true,
            },
        );

        assert_eq!(
            sh.pop_output(),
            Some(Output::SendRemote(
                None,
                SourceHint::Unregister {
                    source: remote1_node_id,
                    to_root: true
                }
            ))
        );
        assert_eq!(
            sh.pop_output(),
            Some(Output::SendRemote(
                Some(remote2),
                SourceHint::Unregister {
                    source: remote1_node_id,
                    to_root: false
                }
            ))
        );
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn register_resend_after_tick() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote_node_id = 2;
        let remote_session_id = 4321;

        //fake a local source with a remote subscribe
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Register);
        sh.on_remote(0, remote, SourceHint::Subscribe(remote_session_id));

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Register { source: node_id, to_root: true })));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote), SourceHint::SubscribeOk(remote_session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote), SourceHint::Sources(vec![node_id]))));
        assert_eq!(sh.pop_output(), None);

        sh.on_tick(1000);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Register { source: node_id, to_root: true })));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote), SourceHint::Register { source: node_id, to_root: false })));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn subscribe_resend_after_tick() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        //subscribe should send a subscribe message
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        sh.on_tick(1000);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn remote_source_timeout() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        //subscribe should send a subscribe message and local source event
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote_node_id = 2;

        sh.on_remote(
            0,
            remote,
            SourceHint::Register {
                source: remote_node_id,
                to_root: true,
            },
        );

        assert_eq!(
            sh.pop_output(),
            Some(Output::SendRemote(
                None,
                SourceHint::Register {
                    source: remote_node_id,
                    to_root: true
                }
            ))
        );
        assert_eq!(sh.pop_output(), Some(Output::SubscribeSource(vec![FeatureControlActor::Controller], remote_node_id)));
        assert_eq!(sh.pop_output(), None);

        assert_eq!(sh.remote_sources.len(), 1);
        sh.on_tick(1000);
        assert_eq!(sh.remote_sources.len(), 1);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        sh.on_tick(TIMEOUT_MS);
        assert_eq!(sh.remote_sources.len(), 0);
        assert_eq!(sh.pop_output(), Some(Output::UnsubscribeSource(vec![FeatureControlActor::Controller], remote_node_id)));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn remote_source_notify_timeout() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        //subscribe should send a subscribe message and local source event
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote_node_id = 2;

        sh.on_remote(0, remote, SourceHint::SubscribeOk(session_id));
        sh.on_remote(
            0,
            remote,
            SourceHint::Register {
                source: remote_node_id,
                to_root: false,
            },
        );

        assert_eq!(sh.pop_output(), Some(Output::SubscribeSource(vec![FeatureControlActor::Controller], remote_node_id)));
        assert_eq!(sh.pop_output(), None);

        assert_eq!(sh.remote_sources.len(), 1);
        sh.on_tick(1000);
        assert_eq!(sh.remote_sources.len(), 1);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        sh.on_tick(TIMEOUT_MS);
        assert_eq!(sh.remote_sources.len(), 0);
        assert_eq!(sh.pop_output(), Some(Output::UnsubscribeSource(vec![FeatureControlActor::Controller], remote_node_id)));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn remote_subscriber_timeout() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        let remote = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote_session_id = 4321;

        sh.on_remote(0, remote, SourceHint::Subscribe(remote_session_id));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(Some(remote), SourceHint::SubscribeOk(remote_session_id))));
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        assert_eq!(sh.remote_subscribers.len(), 1);
        sh.on_tick(1000);
        assert_eq!(sh.remote_subscribers.len(), 1);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        sh.on_tick(TIMEOUT_MS);
        assert_eq!(sh.remote_subscribers.len(), 0);
        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Unsubscribe(session_id))));
        assert_eq!(sh.pop_output(), None);
    }

    #[test]
    fn next_hop_changed() {
        let node_id = 1;
        let session_id = 1234;
        let mut sh = SourceHintLogic::new(node_id, session_id);

        //subscribe should send a subscribe message and local source event
        sh.on_local(0, FeatureControlActor::Controller, LocalCmd::Subscribe);

        assert_eq!(sh.pop_output(), Some(Output::SendRemote(None, SourceHint::Subscribe(session_id))));
        assert_eq!(sh.pop_output(), None);

        let remote1 = SocketAddr::new([127, 0, 0, 1].into(), 1234);
        let remote2 = SocketAddr::new([127, 0, 0, 2].into(), 1234);

        sh.on_remote(0, remote1, SourceHint::SubscribeOk(session_id));

        let source_id = 100;
        sh.on_remote(0, remote1, SourceHint::Register { source: source_id, to_root: false });

        assert_eq!(sh.pop_output(), Some(Output::SubscribeSource(vec![FeatureControlActor::Controller], source_id)));
        assert_eq!(sh.pop_output(), None);

        //now next hop changed to remote2
        sh.on_remote(1000, remote2, SourceHint::SubscribeOk(session_id));

        //then we will reject any message from remote1
        sh.on_remote(0, remote1, SourceHint::Unregister { source: source_id, to_root: false });
        assert_eq!(sh.pop_output(), None);

        //we only accept from remote2
        sh.on_remote(0, remote2, SourceHint::Unregister { source: source_id, to_root: false });
        assert_eq!(sh.pop_output(), Some(Output::UnsubscribeSource(vec![FeatureControlActor::Controller], source_id)));
        assert_eq!(sh.pop_output(), None);
    }
}

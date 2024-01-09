use std::collections::{hash_map::Entry, HashMap, VecDeque};

use atm0s_sdn_identity::NodeId;

use crate::{
    msg::{BroadcastMsg, DirectMsg},
    sdk::{NodeAliasError, NodeAliasResult},
    NodeAliasId,
};

const FIND_ALIAS_TIMEOUT: u64 = 1000;
const SCAN_ALIAS_TIMEOUT: u64 = 5000;

#[derive(Debug, Clone, PartialEq)]
pub struct AliasHistory {
    node: NodeId,
    last_seen: u64,
}

impl PartialOrd for AliasHistory {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.node.partial_cmp(&other.node) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.last_seen.partial_cmp(&other.last_seen)
    }
}

pub enum AliasFindingState {
    Ping {
        started_at: u64,
        waits: Vec<Box<dyn FnOnce(Result<NodeAliasResult, NodeAliasError>) + Send>>,
    },
    Scan {
        started_at: u64,
        waits: Vec<Box<dyn FnOnce(Result<NodeAliasResult, NodeAliasError>) + Send>>,
    },
}

pub struct AliasSlot {
    /// When the alias was registered as local
    local_at: Option<u64>,
    /// Last time we saw this alias on the network
    remote_hint: Option<AliasHistory>,
}

#[derive(Debug, PartialEq)]
pub enum ServiceInternalAction {
    Unicast(NodeId, DirectMsg),
    Broadcast(BroadcastMsg),
}

pub struct ServiceInternal {
    node_id: NodeId,
    aliases: HashMap<NodeAliasId, AliasSlot>,
    finding_aliases: HashMap<NodeAliasId, AliasFindingState>,
    action: VecDeque<ServiceInternalAction>,
}

impl ServiceInternal {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            aliases: HashMap::new(),
            finding_aliases: HashMap::new(),
            action: VecDeque::new(),
        }
    }

    /// adding the alias as local
    pub fn register(&mut self, now_ms: u64, alias: NodeAliasId) {
        match self.aliases.entry(alias.clone()) {
            Entry::Occupied(mut entry) => {
                log::info!("[ServiceInternal {}] Registering a local alias {} that was already registered as remote", self.node_id, entry.key());
                entry.get_mut().local_at = Some(now_ms);
            }
            Entry::Vacant(entry) => {
                log::info!("[ServiceInternal {}] Registering a new alias {} as local", self.node_id, entry.key());
                entry.insert(AliasSlot {
                    local_at: Some(now_ms),
                    remote_hint: None,
                });
            }
        }
        self.action.push_back(ServiceInternalAction::Broadcast(BroadcastMsg::Register(alias)));
    }

    pub fn unregister(&mut self, _now_ms: u64, alias: &NodeAliasId) {
        if let Some(slot) = self.aliases.get_mut(&alias) {
            if slot.local_at.is_some() {
                slot.local_at = None;
                if slot.remote_hint.is_none() {
                    log::info!("[ServiceInternal {}] Unregistering a local alias {} => removed", self.node_id, alias);
                    self.aliases.remove(&alias);
                } else {
                    log::info!("[ServiceInternal {}] Unregistering a local alias {} that was registered as remote", self.node_id, alias);
                }
                self.action.push_back(ServiceInternalAction::Broadcast(BroadcastMsg::Unregister(alias.clone())));
            }
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        // check if alias finding timeout
        let mut to_remove = Vec::new();
        for (alias, finding) in self.finding_aliases.iter_mut() {
            match finding {
                AliasFindingState::Ping { started_at, waits } => {
                    if now_ms >= *started_at + FIND_ALIAS_TIMEOUT {
                        log::info!("[ServiceInternal {}] Alias {} finding timeout => trying SCAN", self.node_id, alias);
                        *finding = AliasFindingState::Scan {
                            started_at: now_ms,
                            waits: waits.drain(..).collect(),
                        };
                        self.action.push_back(ServiceInternalAction::Broadcast(BroadcastMsg::Query(alias.clone())));
                    }
                }
                AliasFindingState::Scan { started_at, waits } => {
                    if now_ms >= *started_at + SCAN_ALIAS_TIMEOUT {
                        log::info!("[ServiceInternal {}] Alias {} scan timeout => not found", self.node_id, alias);
                        to_remove.push(alias.clone());
                        waits.drain(..).for_each(|wait| wait(Err(NodeAliasError::Timeout)));
                    }
                }
            }
        }
        to_remove.drain(..).for_each(|alias| {
            self.finding_aliases.remove(&alias);
        });
    }

    /// First find in local if not found then PING hint node, if not found SCAN by broadcast
    pub fn find_alias(&mut self, now_ms: u64, alias: &NodeAliasId, handler: Box<dyn FnOnce(Result<NodeAliasResult, NodeAliasError>) + Send>) {
        log::info!("[ServiceInternal {}] Find alias {}", self.node_id, alias);
        let (local, remote) = match self.aliases.get(alias) {
            Some(slot) => (slot.local_at.is_some(), slot.remote_hint.as_ref().map(|hint| hint.node)),
            None => (false, None),
        };

        if local {
            log::info!("[ServiceInternal {}] Alias {} found locally", self.node_id, alias);
            handler(Ok(NodeAliasResult::FromLocal));
        } else {
            if let Some(finding) = self.finding_aliases.get_mut(alias) {
                log::info!("[ServiceInternal {}] Alias {} already finding => push to wait queue", self.node_id, alias);
                match finding {
                    AliasFindingState::Ping { waits, .. } => {
                        waits.push(handler);
                    }
                    AliasFindingState::Scan { waits, .. } => {
                        waits.push(handler);
                    }
                }
            } else {
                if let Some(remote_hint_node) = remote {
                    log::info!("[ServiceInternal {}] Alias {} hint node found => trying PING node {}", self.node_id, alias, remote_hint_node);
                    self.finding_aliases.insert(
                        alias.clone(),
                        AliasFindingState::Ping {
                            started_at: now_ms,
                            waits: vec![handler],
                        },
                    );
                    self.action.push_back(ServiceInternalAction::Unicast(remote_hint_node, DirectMsg::Query(alias.clone())));
                } else {
                    log::info!("[ServiceInternal {}] Alias {} hint node not found => trying SCAN", self.node_id, alias);
                    self.finding_aliases.insert(
                        alias.clone(),
                        AliasFindingState::Scan {
                            started_at: now_ms,
                            waits: vec![handler],
                        },
                    );
                    self.action.push_back(ServiceInternalAction::Broadcast(BroadcastMsg::Query(alias.clone())));
                }
            }
        }
    }

    pub fn on_incomming_unicast(&mut self, _now_ms: u64, from: NodeId, msg: DirectMsg) {
        match msg {
            DirectMsg::Response { alias, added_at } => {
                // When we receive a response we update the alias hint if the response is found.
                // We also check if current finding state is PING or SCAN and if so we call the handler if required

                if let Some(added_at) = added_at {
                    log::info!("[ServiceInternal {}] update alias {} hint to {}", self.node_id, alias, from);
                    self.aliases.entry(alias.clone()).or_insert(AliasSlot { local_at: None, remote_hint: None }).remote_hint = Some(AliasHistory { node: from, last_seen: added_at });
                }

                if let Some(finding) = self.finding_aliases.get_mut(&alias) {
                    match finding {
                        AliasFindingState::Ping { started_at, waits } => {
                            if added_at.is_some() {
                                log::info!("[ServiceInternal {}] Alias {} found by PING at {}", self.node_id, alias, from);
                                waits.drain(..).for_each(|wait| wait(Ok(NodeAliasResult::FromHint(from))));
                                self.finding_aliases.remove(&alias);
                            } else {
                                *finding = AliasFindingState::Scan {
                                    started_at: *started_at,
                                    waits: waits.drain(..).collect(),
                                };
                                self.action.push_back(ServiceInternalAction::Broadcast(BroadcastMsg::Query(alias)));
                            }
                        }
                        AliasFindingState::Scan { started_at: _, waits } => {
                            if added_at.is_some() {
                                log::info!("[ServiceInternal {}] Alias {} found by SCAN at {}", self.node_id, alias, from);
                                waits.drain(..).for_each(|wait| wait(Ok(NodeAliasResult::FromScan(from))));
                                self.finding_aliases.remove(&alias);
                            }
                        }
                    }
                }
            }
            DirectMsg::Query(alias) => {
                let added_at = self.aliases.get(&alias).and_then(|slot| slot.local_at);
                self.action.push_back(ServiceInternalAction::Unicast(from, DirectMsg::Response { alias, added_at }));
            }
        }
    }

    pub fn on_incomming_broadcast(&mut self, now_ms: u64, from: NodeId, msg: BroadcastMsg) {
        match msg {
            BroadcastMsg::Register(alias) => {
                // save the alias as remote
                match self.aliases.entry(alias) {
                    Entry::Occupied(mut entry) => {
                        log::info!("[ServiceInternal {}] Registering a remote alias {} that was already registered", self.node_id, entry.key());
                        entry.get_mut().remote_hint = Some(AliasHistory { node: from, last_seen: now_ms });
                    }
                    Entry::Vacant(entry) => {
                        log::info!("[ServiceInternal {}] Registering a new alias {} as remote", self.node_id, entry.key());
                        entry.insert(AliasSlot {
                            local_at: None,
                            remote_hint: Some(AliasHistory { node: from, last_seen: now_ms }),
                        });
                    }
                }
            }
            BroadcastMsg::Unregister(alias) => {
                // clear alias hint if from same node
                if let Some(slot) = self.aliases.get_mut(&alias) {
                    if slot.remote_hint.as_ref().map(|hint| hint.node == from).unwrap_or(false) {
                        log::info!("[ServiceInternal {}] Unregistering a remote alias {} => removed hint", self.node_id, alias);
                        slot.remote_hint = None;
                    } else {
                        log::info!(
                            "[ServiceInternal {}] Ignore unregistering a remote alias {} but from node difference with last hint",
                            self.node_id,
                            alias
                        );
                    }
                } else {
                    log::warn!("[ServiceInternal {}] Unregistering an unknown alias {}", self.node_id, alias);
                }
            }
            BroadcastMsg::Query(alias) => {
                let added_at = self.aliases.get(&alias).and_then(|slot| slot.local_at);
                log::info!("[ServiceInternal {}] On Alias ({}) Query, local {}", self.node_id, alias, added_at.is_some());
                self.action.push_back(ServiceInternalAction::Unicast(from, DirectMsg::Response { alias, added_at }));
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<ServiceInternalAction> {
        self.action.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use crate::{
        internal::{ServiceInternalAction, FIND_ALIAS_TIMEOUT, SCAN_ALIAS_TIMEOUT},
        msg::{BroadcastMsg, DirectMsg},
        NodeAliasError, NodeAliasId, NodeAliasResult,
    };

    use super::ServiceInternal;

    #[test]
    fn register_local() {
        let node_id = 1000;
        let mut internal = ServiceInternal::new(node_id);
        let alias: NodeAliasId = 666.into();

        internal.register(0, alias.clone());
        assert_eq!(internal.aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(super::ServiceInternalAction::Broadcast(super::BroadcastMsg::Register(alias.clone()))));

        //re-register should fire more broadcast
        internal.register(0, alias.clone());
        assert_eq!(internal.aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(super::ServiceInternalAction::Broadcast(super::BroadcastMsg::Register(alias.clone()))));

        //unregister should fire broadcast
        internal.unregister(0, &alias);
        assert_eq!(internal.aliases.len(), 0);
        assert_eq!(internal.pop_action(), Some(super::ServiceInternalAction::Broadcast(super::BroadcastMsg::Unregister(alias.clone()))));

        //unregister twice should not fire broadcast
        internal.unregister(0, &alias);
        assert_eq!(internal.pop_action(), None);
    }

    #[test]
    fn find_alias_scan_found() {
        let node_id = 1000;
        let remote_node_id = 2000;
        let mut internal = ServiceInternal::new(node_id);
        let alias: NodeAliasId = 666.into();

        let res = Arc::new(Mutex::new(None));
        let res_clone = res.clone();
        internal.find_alias(
            0,
            &alias,
            Box::new(move |res| {
                *res_clone.lock() = Some(res);
            }),
        );

        assert_eq!(internal.finding_aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(ServiceInternalAction::Broadcast(BroadcastMsg::Query(alias.clone()))));

        internal.on_incomming_unicast(
            0,
            remote_node_id,
            DirectMsg::Response {
                alias: alias.clone(),
                added_at: Some(10),
            },
        );
        assert_eq!(internal.finding_aliases.len(), 0);
        assert_eq!(internal.pop_action(), None);
        assert_eq!(*res.lock(), Some(Ok(NodeAliasResult::FromScan(remote_node_id))));
    }

    #[test]
    fn find_alias_scan_not_found() {
        let node_id = 1000;
        let mut internal = ServiceInternal::new(node_id);
        let alias: NodeAliasId = 666.into();

        let res = Arc::new(Mutex::new(None));
        let res_clone = res.clone();
        internal.find_alias(
            0,
            &alias,
            Box::new(move |res| {
                *res_clone.lock() = Some(res);
            }),
        );

        assert_eq!(internal.finding_aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(ServiceInternalAction::Broadcast(BroadcastMsg::Query(alias.clone()))));

        internal.on_tick(SCAN_ALIAS_TIMEOUT);
        assert_eq!(internal.finding_aliases.len(), 0);
        assert_eq!(internal.pop_action(), None);
        assert_eq!(*res.lock(), Some(Err(NodeAliasError::Timeout)));
    }

    #[test]
    fn find_alias_find_found() {
        let node_id = 1000;
        let remote_node_id = 2000;
        let mut internal = ServiceInternal::new(node_id);
        let alias: NodeAliasId = 666.into();

        internal.on_incomming_broadcast(0, remote_node_id, BroadcastMsg::Register(alias.clone()));

        let res = Arc::new(Mutex::new(None));
        let res_clone = res.clone();
        internal.find_alias(
            0,
            &alias,
            Box::new(move |res| {
                *res_clone.lock() = Some(res);
            }),
        );

        assert_eq!(internal.finding_aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(ServiceInternalAction::Unicast(remote_node_id, DirectMsg::Query(alias.clone()))));

        internal.on_incomming_unicast(
            0,
            remote_node_id,
            DirectMsg::Response {
                alias: alias.clone(),
                added_at: Some(10),
            },
        );
        assert_eq!(internal.finding_aliases.len(), 0);
        assert_eq!(internal.pop_action(), None);
        assert_eq!(*res.lock(), Some(Ok(NodeAliasResult::FromHint(remote_node_id))));
    }

    #[test]
    fn find_alias_find_not_found() {
        let node_id = 1000;
        let remote_node_id = 2000;
        let mut internal = ServiceInternal::new(node_id);
        let alias: NodeAliasId = 666.into();

        internal.on_incomming_broadcast(0, remote_node_id, BroadcastMsg::Register(alias.clone()));

        let res = Arc::new(Mutex::new(None));
        let res_clone = res.clone();
        internal.find_alias(
            0,
            &alias,
            Box::new(move |res| {
                *res_clone.lock() = Some(res);
            }),
        );

        assert_eq!(internal.finding_aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(ServiceInternalAction::Unicast(remote_node_id, DirectMsg::Query(alias.clone()))));

        internal.on_incomming_unicast(0, remote_node_id, DirectMsg::Response { alias: alias.clone(), added_at: None });
        assert_eq!(internal.finding_aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(ServiceInternalAction::Broadcast(BroadcastMsg::Query(alias.clone()))));

        internal.on_tick(SCAN_ALIAS_TIMEOUT);
        assert_eq!(internal.finding_aliases.len(), 0);
        assert_eq!(internal.pop_action(), None);
        assert_eq!(*res.lock(), Some(Err(NodeAliasError::Timeout)));
    }

    #[test]
    fn find_alias_find_switch_scan_after_timeout() {
        let node_id = 1000;
        let remote_node_id = 2000;
        let mut internal = ServiceInternal::new(node_id);
        let alias: NodeAliasId = 666.into();

        internal.on_incomming_broadcast(0, remote_node_id, BroadcastMsg::Register(alias.clone()));

        let res = Arc::new(Mutex::new(None));
        let res_clone = res.clone();
        internal.find_alias(
            0,
            &alias,
            Box::new(move |res| {
                *res_clone.lock() = Some(res);
            }),
        );

        assert_eq!(internal.finding_aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(ServiceInternalAction::Unicast(remote_node_id, DirectMsg::Query(alias.clone()))));

        internal.on_tick(FIND_ALIAS_TIMEOUT);

        assert_eq!(internal.finding_aliases.len(), 1);
        assert_eq!(internal.pop_action(), Some(ServiceInternalAction::Broadcast(BroadcastMsg::Query(alias.clone()))));
    }
}

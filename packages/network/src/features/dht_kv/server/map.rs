use std::collections::{HashMap, VecDeque};

use crate::features::dht_kv::msg::{ClientMapCommand, Key, NodeSession, ServerMapEvent, Version};

const RESEND_MS: u64 = 200; //We will resend set or del command if we don't get ack in this time
const TIMEOUT_MS: u64 = 10000; //We will remove sub if we don't get any message from it in this time

enum MapSlot {
    Unspecific,
    Set { data: Vec<u8>, version: Version, live_at: u64 },
}

impl MapSlot {
    fn new() -> Self {
        Self::Unspecific
    }

    fn set(&mut self, now: u64, new_version: Version, new_data: Vec<u8>) -> bool {
        match self {
            MapSlot::Unspecific => {
                *self = MapSlot::Set {
                    data: new_data,
                    version: new_version,
                    live_at: now,
                };
                true
            }
            MapSlot::Set { version, data, live_at } => {
                if version.0 < new_version.0 {
                    *version = new_version;
                    *data = new_data;
                    *live_at = now;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn del(&mut self, _now: u64, version: Version) -> Option<Version> {
        match self {
            MapSlot::Unspecific => None,
            MapSlot::Set { version: current_version, .. } => {
                if version.0 >= current_version.0 {
                    let del_version = *current_version;
                    *self = MapSlot::Unspecific;
                    Some(del_version)
                } else {
                    None
                }
            }
        }
    }

    fn dump(&self) -> Option<(Version, Vec<u8>)> {
        match self {
            MapSlot::Unspecific => None,
            MapSlot::Set { version, data, .. } => Some((*version, data.clone())),
        }
    }
}

struct WaitAcksEvent {
    event: ServerMapEvent,
    remotes: Vec<NodeSession>,
    last_send_ms: u64,
}

struct SubSlot {
    id: u64,
    last_ts: u64,
}

pub struct RemoteMap {
    session: NodeSession,
    slots: HashMap<(Key, NodeSession), MapSlot>,
    slots_event: HashMap<(Key, NodeSession), WaitAcksEvent>,
    subs: HashMap<NodeSession, SubSlot>,
    queue: VecDeque<(NodeSession, ServerMapEvent)>,
}

impl RemoteMap {
    pub fn new(session: NodeSession) -> Self {
        Self {
            session,
            slots: HashMap::new(),
            slots_event: HashMap::new(),
            subs: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        //clean-up timeout subs
        let mut to_remove = vec![];
        for (node, slot) in self.subs.iter() {
            if now >= slot.last_ts + TIMEOUT_MS {
                log::debug!("[ServerMap] Remove sub from {} after timeout {TIMEOUT_MS}", node.0);
                to_remove.push(*node);
            }
        }

        for node in to_remove {
            self.subs.remove(&node);
        }

        //resend events
        let mut to_remove = vec![];
        for (key, slot) in self.slots_event.iter_mut() {
            if now >= TIMEOUT_MS + slot.last_send_ms {
                log::warn!("[ServerMap] Remove wait event {:?} for key {} to node {} after timeout {TIMEOUT_MS}", slot.event, key.0, key.1 .0);
                to_remove.push(*key);
            } else if now >= RESEND_MS + slot.last_send_ms {
                for remote in &slot.remotes {
                    log::debug!("[ServerMap] Resend event {:?} for key {} to node {} after timeout {RESEND_MS}", slot.event, key.0, remote.0);
                    self.queue.push_back((*remote, slot.event.clone()));
                    slot.last_send_ms = now;
                }
            }
        }

        for key in to_remove {
            self.slots_event.remove(&key);
        }
    }

    pub fn dump(&self) -> Vec<(Key, NodeSession, Version, Vec<u8>)> {
        self.slots
            .iter()
            .filter_map(|((key, session), slot)| {
                if let Some((version, data)) = slot.dump() {
                    Some((*key, *session, version, data))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn on_client(&mut self, now: u64, remote: NodeSession, cmd: ClientMapCommand) -> Option<ServerMapEvent> {
        match cmd {
            ClientMapCommand::Set(key, version, data) => {
                let slot = self.get_slot(key, remote, true).expect("must have slot with auto_create");
                if slot.set(now, version, data.clone()) {
                    log::debug!("[ServerMap] Set key {} from {} with version {}", key, remote.0, version.0);
                    self.fire_event(now, key, remote, ServerMapEvent::OnSet { key, version, source: remote, data });
                    Some(ServerMapEvent::SetOk(key, version))
                } else {
                    log::warn!("[ServerMap] Set key {} from {} with version {} failed", key, remote.0, version.0);
                    None
                }
            }
            ClientMapCommand::Del(key, req_version) => {
                let slot = self.get_slot(key, remote, false)?;
                if let Some(version) = slot.del(now, req_version) {
                    log::debug!("[ServerMap] Del key {} from {} with req_ver {req_version}, in_store {version}", key, remote.0);
                    self.slots.remove(&(key, remote));
                    self.fire_event(now, key, remote, ServerMapEvent::OnDel { key, version, source: remote });
                    Some(ServerMapEvent::DelOk(key, version))
                } else {
                    log::warn!("[ServerMap] Del key {} from {} with req_ver {req_version} failed", key, remote.0);
                    None
                }
            }
            ClientMapCommand::Sub(id, locked_session) => {
                let old = self.subs.insert(remote, SubSlot { last_ts: now, id });
                if old.is_none() || locked_session != Some(self.session) {
                    log::debug!("[ServerMap] New sub from {} with id {}", remote.0, id);
                    self.fire_sub_events(now, remote);
                }
                Some(ServerMapEvent::SubOk(id))
            }
            ClientMapCommand::Unsub(id) => {
                let sub = self.subs.get(&remote)?;
                if sub.id == id {
                    log::debug!("[ServerMap] Unsub from {} with id {}", remote.0, id);
                    self.subs.remove(&remote);
                    Some(ServerMapEvent::UnsubOk(id))
                } else {
                    log::warn!("[ServerMap] Unsub from {} failed, wrong id {} vs instore id {}", remote.0, id, sub.id);
                    None
                }
            }
            ClientMapCommand::OnSetAck(key, session, acked_version) => {
                let slot = self.slots_event.get_mut(&(key, session))?;
                if let ServerMapEvent::OnSet { version, .. } = &slot.event {
                    if acked_version == *version {
                        log::debug!("[ServerMap] Acked set key {key} from {} with version {acked_version}", remote.0);
                        slot.remotes.retain(|r| *r != remote);
                        if slot.remotes.is_empty() {
                            log::debug!("[ServerMap] Remove wait event OnSet for key {key} after all remotes acked");
                            self.slots_event.remove(&(key, session));
                        }
                    } else {
                        log::warn!(
                            "[ServerMap] Acked set key {key} from {} with version {acked_version} not match with in-store version {version}",
                            remote.0
                        );
                    }
                } else {
                    log::warn!("[ServerMap] Acked set key {key} from {} not match with in-store event type", remote.0);
                }
                None
            }
            ClientMapCommand::OnDelAck(key, session, acked_version) => {
                let slot = self.slots_event.get_mut(&(key, session))?;
                if let ServerMapEvent::OnDel { version, .. } = &slot.event {
                    if acked_version == *version {
                        log::debug!("[ServerMap] Acked del key {key} from {} with version {acked_version}", remote.0);
                        slot.remotes.retain(|r| *r != remote);
                        if slot.remotes.is_empty() {
                            log::debug!("[ServerMap] Remove wait event OnDel for key {key} after all remotes acked");
                            self.slots_event.remove(&(key, session));
                        }
                    } else {
                        log::warn!(
                            "[ServerMap] Acked del key {key} from {} with version {acked_version} not match with in-store version {version}",
                            remote.0
                        );
                    }
                } else {
                    log::warn!("[ServerMap] Acked del key {key} from {} not match with in-store event type", remote.0);
                }
                None
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<(NodeSession, ServerMapEvent)> {
        self.queue.pop_front()
    }

    pub fn should_clean(&self) -> bool {
        self.slots.is_empty() && self.subs.is_empty() && self.slots_event.is_empty()
    }

    fn get_slot(&mut self, key: Key, source: NodeSession, auto_create: bool) -> Option<&mut MapSlot> {
        if !self.slots.contains_key(&(key, source)) && auto_create {
            log::debug!("[ServerMap] Create new slot for key {key} from node {}", source.0);
            self.slots.insert((key, source), MapSlot::new());
        }
        self.slots.get_mut(&(key, source))
    }

    /// Because source already has this data, then we only fire to other nodes
    fn fire_event(&mut self, now: u64, key: Key, source: NodeSession, event: ServerMapEvent) {
        if self.subs.is_empty() {
            return;
        }
        let mut remotes = vec![];
        for (remote, _) in &self.subs {
            if *remote != source {
                log::debug!("[ServerMap] Fire event {:?} for key {key} to {}", event, remote.0);
                remotes.push(*remote);
                self.queue.push_back((*remote, event.clone()));
            }
        }
        self.slots_event.insert((key, source), WaitAcksEvent { event, remotes, last_send_ms: now });
    }

    /// We only send events which not owned by remote
    fn fire_sub_events(&mut self, now: u64, remote: NodeSession) {
        for (key, slot) in self.slots.iter() {
            if key.1 == remote {
                continue;
            }
            if let Some((version, data)) = slot.dump() {
                log::debug!("[ServerMap] Fire event OnSet for key {} to {}", key.0, remote.0);
                let event = ServerMapEvent::OnSet {
                    key: key.0,
                    version,
                    source: key.1,
                    data,
                };
                let entry = self.slots_event.entry(*key).or_insert_with(|| WaitAcksEvent {
                    event: event.clone(),
                    remotes: vec![],
                    last_send_ms: now,
                });
                if !entry.remotes.contains(&remote) {
                    entry.remotes.push(remote);
                }
                self.queue.push_back((remote, event));
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{MapSlot, RemoteMap};
    use crate::features::dht_kv::{
        msg::{ClientMapCommand, Key, NodeSession, ServerMapEvent, Version},
        server::map::RESEND_MS,
    };

    #[test]
    fn map_slot_set_del_corect() {
        let mut slot = MapSlot::new();

        assert_eq!(slot.set(0, Version(0), vec![1, 2, 3]), true);
        assert_eq!(slot.dump(), Some((Version(0), vec![1, 2, 3])));
        assert_eq!(slot.set(0, Version(1), vec![1, 2, 4]), true);
        assert_eq!(slot.dump(), Some((Version(1), vec![1, 2, 4])));
        assert_eq!(slot.del(0, Version(1)), Some(Version(1)));
        assert_eq!(slot.dump(), None);
    }

    #[test]
    fn map_slot_set_del_newer_version_corect() {
        let mut slot = MapSlot::new();

        assert_eq!(slot.set(0, Version(0), vec![1, 2, 3]), true);
        assert_eq!(slot.dump(), Some((Version(0), vec![1, 2, 3])));
        assert_eq!(slot.del(0, Version(100)), Some(Version(0)));
        assert_eq!(slot.dump(), None);
    }

    #[test]
    fn map_slot_set_del_invalid() {
        let mut slot = MapSlot::new();

        assert_eq!(slot.set(0, Version(100), vec![1, 2, 3]), true);
        assert_eq!(slot.dump(), Some((Version(100), vec![1, 2, 3])));
        assert_eq!(slot.set(0, Version(1), vec![1, 2, 4]), false);
        assert_eq!(slot.dump(), Some((Version(100), vec![1, 2, 3])));
        assert_eq!(slot.del(0, Version(1)), None);
        assert_eq!(slot.dump(), Some((Version(100), vec![1, 2, 3])));
    }

    #[test]
    fn map_correct_set_update_del_event() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(2), vec![1, 2, 3, 5])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(2)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(2),
                    source,
                    data: vec![1, 2, 3, 5]
                }
            ))
        );

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Del(Key(1000), Version(2))),
            Some(ServerMapEvent::DelOk(Key(1000), Version(2)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnDel {
                    key: Key(1000),
                    version: Version(2),
                    source
                }
            ))
        );
    }

    #[test]
    fn map_correct_sub_after_set_event() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_client(1, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );

        //if resub with same session, it should not fire event
        assert_eq!(map.on_client(2, consumer, ClientMapCommand::Sub(2, Some(relay))), Some(ServerMapEvent::SubOk(2)));
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_correct_sub_new_relay() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_client(1, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );

        //if resub with new session, it should fire event
        let old_relay = NodeSession(0, 1);

        assert_eq!(map.on_client(2, consumer, ClientMapCommand::Sub(2, Some(old_relay))), Some(ServerMapEvent::SubOk(2)));
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_correct_unsub() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);
        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Unsub(1)), Some(ServerMapEvent::UnsubOk(1)));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_invalid_unsub() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);
        //unsub with other sub-id with not affected
        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Unsub(100)), None);
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_invalid_set() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);

        //set with same version should not affected
        assert_eq!(map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])), None);
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_invalid_del() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);

        //del with older version should not affected
        assert_eq!(map.on_client(0, source, ClientMapCommand::Del(Key(1000), Version(0))), None);
        assert_eq!(map.pop_action(), None);
        assert_eq!(map.slots.len(), 1);
    }

    #[test]
    fn map_del_with_newer_version_should_work() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);

        //del with newer version should affected
        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Del(Key(1000), Version(2))),
            Some(ServerMapEvent::DelOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnDel {
                    key: Key(1000),
                    version: Version(1),
                    source
                }
            ))
        );
        assert_eq!(map.pop_action(), None);
        assert_eq!(map.slots.len(), 0);
    }

    #[test]
    fn map_event_should_resend_before_ack() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);

        map.on_tick(RESEND_MS);

        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_event_should_resend_before_ack_with_after_sub() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);

        map.on_tick(RESEND_MS);

        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_event_should_not_resend_after_ack() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);
        let consumer = NodeSession(5, 6);

        assert_eq!(map.on_client(0, consumer, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(
            map.pop_action(),
            Some((
                consumer,
                ServerMapEvent::OnSet {
                    key: Key(1000),
                    version: Version(1),
                    source,
                    data: vec![1, 2, 3, 4]
                }
            ))
        );
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_client(2, consumer, ClientMapCommand::OnSetAck(Key(1000), source, Version(1))), None);

        map.on_tick(RESEND_MS);

        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_event_should_not_send_to_source() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);

        assert_eq!(map.on_client(0, source, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_event_should_not_send_to_source_after_set() {
        let relay = NodeSession(1, 2);
        let mut map = RemoteMap::new(relay);

        let source = NodeSession(3, 4);

        assert_eq!(
            map.on_client(0, source, ClientMapCommand::Set(Key(1000), Version(1), vec![1, 2, 3, 4])),
            Some(ServerMapEvent::SetOk(Key(1000), Version(1)))
        );
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_client(0, source, ClientMapCommand::Sub(1, None)), Some(ServerMapEvent::SubOk(1)));
        assert_eq!(map.pop_action(), None);
    }
}

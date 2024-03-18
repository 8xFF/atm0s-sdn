use std::collections::{HashMap, VecDeque};

use crate::features::dht_kv::msg::{ClientMapCommand, NodeSession, ServerMapEvent, SubKey, Version};

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

    fn del(&mut self, _now: u64, version: Version) -> bool {
        match self {
            MapSlot::Unspecific => false,
            MapSlot::Set { version: current_version, .. } => {
                if version.0 >= current_version.0 {
                    *self = MapSlot::Unspecific;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn get_event(&self, sub: SubKey, source: NodeSession) -> Option<ServerMapEvent> {
        match self {
            MapSlot::Unspecific => None,
            MapSlot::Set { version, data, .. } => Some(ServerMapEvent::OnSet {
                sub,
                version: *version,
                source,
                data: data.clone(),
            }),
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
    slots: HashMap<(SubKey, NodeSession), MapSlot>,
    slots_event: HashMap<(SubKey, NodeSession), WaitAcksEvent>,
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
                to_remove.push(*node);
            }
        }

        for node in to_remove {
            self.subs.remove(&node);
        }

        //resend events
        let mut to_remove = vec![];
        for (sub, slot) in self.slots_event.iter_mut() {
            if now >= TIMEOUT_MS + slot.last_send_ms {
                to_remove.push(*sub);
            } else if now >= RESEND_MS + slot.last_send_ms {
                for remote in &slot.remotes {
                    self.queue.push_back((*remote, slot.event.clone()));
                    slot.last_send_ms = now;
                }
            }
        }

        for sub in to_remove {
            self.slots_event.remove(&sub);
        }
    }

    pub fn dump(&self) -> Vec<(SubKey, NodeSession, Version, Vec<u8>)> {
        self.slots
            .iter()
            .filter_map(|((sub, session), slot)| {
                if let Some((version, data)) = slot.dump() {
                    Some((*sub, *session, version, data))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn on_client(&mut self, now: u64, remote: NodeSession, cmd: ClientMapCommand) -> Option<ServerMapEvent> {
        match cmd {
            ClientMapCommand::Set(sub, version, data) => {
                let slot = self.get_slot(sub, remote, true).expect("must have slot with auto_create");
                if slot.set(now, version, data.clone()) {
                    self.fire_event(now, sub, remote, ServerMapEvent::OnSet { sub, version, source: remote, data });
                    Some(ServerMapEvent::SetOk(sub, version))
                } else {
                    None
                }
            }
            ClientMapCommand::Del(sub, version) => {
                let slot = self.get_slot(sub, remote, false)?;
                if slot.del(now, version) {
                    self.fire_event(now, sub, remote, ServerMapEvent::OnDel { sub, version, source: remote });
                    Some(ServerMapEvent::DelOk(sub, version))
                } else {
                    None
                }
            }
            ClientMapCommand::Sub(id, locked_session) => {
                let old = self.subs.insert(remote, SubSlot { last_ts: now, id });
                if old.is_none() || locked_session != Some(self.session) {
                    self.fire_sub_events(now, remote);
                }
                Some(ServerMapEvent::SubOk(id))
            }
            ClientMapCommand::Unsub(id) => {
                let sub = self.subs.get(&remote)?;
                if sub.id == id {
                    self.subs.remove(&remote);
                    Some(ServerMapEvent::UnsubOk(id))
                } else {
                    None
                }
            }
            ClientMapCommand::OnSetAck(sub, session, acked_version) => {
                let slot = self.slots_event.get_mut(&(sub, session))?;
                if let ServerMapEvent::OnSet { version, .. } = &slot.event {
                    if acked_version == *version {
                        slot.remotes.retain(|r| *r != remote);
                    }
                }
                if slot.remotes.is_empty() {
                    self.slots_event.remove(&(sub, session));
                }
                None
            }
            ClientMapCommand::OnDelAck(sub, session, acked_version) => {
                let slot = self.slots_event.get_mut(&(sub, session))?;
                if let ServerMapEvent::OnDel { version, .. } = &slot.event {
                    if acked_version == *version {
                        slot.remotes.retain(|r| *r != remote);
                    }
                }
                if slot.remotes.is_empty() {
                    self.slots_event.remove(&(sub, session));
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

    fn get_slot(&mut self, sub: SubKey, source: NodeSession, auto_create: bool) -> Option<&mut MapSlot> {
        if !self.slots.contains_key(&(sub, source)) {
            self.slots.insert((sub, source), MapSlot::new());
        }
        self.slots.get_mut(&(sub, source))
    }

    fn fire_event(&mut self, now: u64, sub: SubKey, source: NodeSession, event: ServerMapEvent) {
        if self.subs.is_empty() {
            return;
        }
        let mut remotes = vec![];
        for (remote, _) in &self.subs {
            self.queue.push_back((*remote, event.clone()));
        }
        self.slots_event.insert((sub, source), WaitAcksEvent { event, remotes, last_send_ms: now });
    }

    fn fire_sub_events(&mut self, now: u64, remote: NodeSession) {
        for (sub, slot) in self.slots.iter() {
            if let Some(event) = slot.get_event(sub.0, sub.1) {
                let entry = self.slots_event.entry(*sub).or_insert_with(|| WaitAcksEvent {
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

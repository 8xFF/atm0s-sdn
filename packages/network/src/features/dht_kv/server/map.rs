use std::collections::{HashMap, VecDeque};

use crate::features::dht_kv::msg::{ClientMapCommand, NodeSession, ServerMapEvent, SubKey, Version};

const RESEND_MS: u64 = 200; //We will resend set or del command if we don't get ack in this time
const TIMEOUT_MS: u64 = 10000; //We will remove sub if we don't get any message from it in this time

enum MapSlot {
    Unspecific,
    Set { data: Vec<u8>, version: Version, locker: NodeSession, live_at: u64 },
}

impl MapSlot {
    fn new() -> Self {
        Self::Unspecific
    }

    fn set(&mut self, now: u64, remote: NodeSession, new_version: Version, new_data: Vec<u8>) -> bool {
        match self {
            MapSlot::Unspecific => {
                *self = MapSlot::Set {
                    data: new_data,
                    version: new_version,
                    locker: remote,
                    live_at: now,
                };
                true
            }
            MapSlot::Set { locker, version, data, live_at } => {
                if locker.0 == remote.0 && locker.1 == remote.1 && version.0 < new_version.0 {
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

    fn del(&mut self, _now: u64, remote: NodeSession, version: Version) -> bool {
        match self {
            MapSlot::Unspecific => false,
            MapSlot::Set { locker, version: current_version, .. } => {
                if locker.0 == remote.0 && locker.1 == remote.1 && version.0 >= current_version.0 {
                    *self = MapSlot::Unspecific;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn dump(&self) -> Option<(Version, NodeSession, Vec<u8>)> {
        match self {
            MapSlot::Unspecific => None,
            MapSlot::Set { version, data, locker, .. } => Some((*version, *locker, data.clone())),
        }
    }
}

struct WaitAcksEvent {
    event: ServerMapEvent,
    remotes: Vec<NodeSession>,
    last_send_ms: u64,
}

struct SubSlot {
    session: u64,
    last_ts: u64,
}

#[derive(Default)]
pub struct RemoteMap {
    slots: HashMap<SubKey, MapSlot>,
    slots_event: HashMap<SubKey, WaitAcksEvent>,
    subs: HashMap<NodeSession, SubSlot>,
    queue: VecDeque<(NodeSession, ServerMapEvent)>,
}

impl RemoteMap {
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

    pub fn dump(&self) -> Vec<(SubKey, Version, NodeSession, Vec<u8>)> {
        self.slots
            .iter()
            .filter_map(|(sub, slot)| {
                if let Some((version, locker, data)) = slot.dump() {
                    Some((*sub, version, locker, data))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn on_client(&mut self, now: u64, remote: NodeSession, cmd: ClientMapCommand) -> Option<ServerMapEvent> {
        match cmd {
            ClientMapCommand::Set(sub, version, data) => {
                let slot = self.get_slot(sub, true).expect("must have slot with auto_create");
                if slot.set(now, remote, version, data.clone()) {
                    self.fire_event(now, sub, ServerMapEvent::OnSet { sub, version, source: remote, data });
                    Some(ServerMapEvent::SetOk(sub, version))
                } else {
                    None
                }
            }
            ClientMapCommand::Del(sub, version) => {
                let slot = self.get_slot(sub, false)?;
                if slot.del(now, remote, version) {
                    self.fire_event(now, sub, ServerMapEvent::OnDel { sub, version, source: remote });
                    Some(ServerMapEvent::DelOk(sub, version))
                } else {
                    None
                }
            }
            ClientMapCommand::Sub(session) => {
                self.subs.insert(remote, SubSlot { last_ts: now, session });
                Some(ServerMapEvent::SubOk(session))
            }
            ClientMapCommand::Unsub(session) => {
                let sub = self.subs.get(&remote)?;
                if sub.session == session {
                    self.subs.remove(&remote);
                    Some(ServerMapEvent::UnsubOk(session))
                } else {
                    None
                }
            }
            ClientMapCommand::OnSetAck(sub, acked_version) => {
                let slot = self.slots_event.get_mut(&sub)?;
                if let ServerMapEvent::OnSet { version, .. } = &slot.event {
                    if acked_version == *version {
                        slot.remotes.retain(|r| *r != remote);
                    }
                }
                if slot.remotes.is_empty() {
                    self.slots_event.remove(&sub);
                }
                None
            }
            ClientMapCommand::OnDelAck(sub, acked_version) => {
                let slot = self.slots_event.get_mut(&sub)?;
                if let ServerMapEvent::OnDel { version, .. } = &slot.event {
                    if acked_version == *version {
                        slot.remotes.retain(|r| *r != remote);
                    }
                }
                if slot.remotes.is_empty() {
                    self.slots_event.remove(&sub);
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

    fn get_slot(&mut self, sub: SubKey, auto_create: bool) -> Option<&mut MapSlot> {
        if !self.slots.contains_key(&sub) {
            self.slots.insert(sub, MapSlot::new());
        }
        self.slots.get_mut(&sub)
    }

    fn fire_event(&mut self, now: u64, sub: SubKey, event: ServerMapEvent) {
        if self.subs.is_empty() {
            return;
        }
        let mut remotes = vec![];
        for (remote, _) in &self.subs {
            self.queue.push_back((*remote, event.clone()));
        }
        self.slots_event.insert(sub, WaitAcksEvent { event, remotes, last_send_ms: now });
    }
}

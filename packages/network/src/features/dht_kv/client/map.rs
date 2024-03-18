use std::collections::{HashMap, VecDeque};

use crate::{
    base::ServiceId,
    features::dht_kv::{
        msg::{ClientMapCommand, NodeSession, ServerMapEvent, Version},
        MapControl, MapEvent, SubKey,
    },
};

const RESEND_MS: u64 = 200; //We will resend set or del command if we don't get ack in this time
const SYNC_MS: u64 = 1500; //We will predict send current state for avoiding out-of-sync, this can be waste of bandwidth but it's better than out-of-sync

enum MapSlot {
    Unspecific {
        sub: SubKey,
    },
    Remote {
        sub: SubKey,
        value: Option<Vec<u8>>,
    },
    Local {
        sub: SubKey,
        value: Option<Vec<u8>>,
        version: Version,
        syncing: bool,
        last_sync: u64,
    },
}

impl MapSlot {
    pub fn new(sub: SubKey) -> Self {
        Self::Unspecific { sub }
    }

    /// This method is called when a new value is set to the map, this will overwrite the old value even if it's not synced or from remote.
    pub fn set(&mut self, now: u64, new_data: Vec<u8>) -> Option<ClientMapCommand> {
        match self {
            MapSlot::Unspecific { sub } | MapSlot::Remote { sub, .. } => {
                let version = Version(now); //TODO use real version
                let sub = *sub;
                *self = MapSlot::Local {
                    sub,
                    value: Some(new_data.clone()),
                    version,
                    syncing: true,
                    last_sync: now,
                };
                Some(ClientMapCommand::Set(sub, version, new_data))
            }
            MapSlot::Local {
                sub,
                value,
                version,
                syncing,
                last_sync,
            } => {
                *value = Some(new_data.clone());
                *version = Version(now); //TODO use real version
                *syncing = true;
                *last_sync = now;
                Some(ClientMapCommand::Set(*sub, *version, new_data))
            }
        }
    }

    /// We only can delete if the slot is local, if the slot is remote, we need to wait for the slot to be removed or timeout.
    pub fn del(&mut self, now: u64) -> Option<ClientMapCommand> {
        match self {
            MapSlot::Unspecific { .. } | MapSlot::Remote { .. } => None,
            MapSlot::Local {
                sub,
                value,
                version,
                syncing,
                last_sync,
                ..
            } => {
                *value = None;
                *syncing = true;
                *last_sync = now;
                Some(ClientMapCommand::Del(*sub, *version))
            }
        }
    }

    pub fn sync(&mut self, now: u64) -> Option<ClientMapCommand> {
        match self {
            MapSlot::Unspecific { .. } | MapSlot::Remote { .. } => None,
            MapSlot::Local {
                sub,
                value,
                version,
                syncing,
                last_sync,
            } => {
                if *syncing && now >= *last_sync + SYNC_MS {
                    *last_sync = now;
                    if let Some(value) = value {
                        Some(ClientMapCommand::Set(*sub, *version, value.clone()))
                    } else {
                        Some(ClientMapCommand::Del(*sub, *version))
                    }
                } else {
                    None
                }
            }
        }
    }

    pub fn set_ok(&mut self, version: Version) {
        match self {
            MapSlot::Unspecific { .. } | MapSlot::Remote { .. } => {}
            MapSlot::Local {
                version: slot_version,
                syncing,
                value,
                ..
            } => {
                if *slot_version == version && value.is_some() {
                    *syncing = false;
                }
            }
        }
    }

    pub fn del_ok(&mut self, version: Version) {
        match self {
            MapSlot::Unspecific { .. } | MapSlot::Remote { .. } => {}
            MapSlot::Local {
                version: slot_version,
                syncing,
                value,
                ..
            } => {
                if *slot_version == version && value.is_none() {
                    *syncing = false;
                }
            }
        }
    }

    pub fn on_set(&mut self, now: u64, sub: SubKey, source: NodeSession, version: Version, data: Vec<u8>) -> Option<ClientMapCommand> {
        match self {
            MapSlot::Unspecific { .. } | MapSlot::Remote { .. } => {
                *self = MapSlot::Remote { sub, value: Some(data) };
                Some(ClientMapCommand::OnSetAck(sub, source, version))
            }
            _ => None,
        }
    }

    pub fn on_del(&mut self, now: u64, sub: SubKey, source: NodeSession, version: Version) -> Option<ClientMapCommand> {
        match self {
            MapSlot::Remote { .. } => {
                *self = MapSlot::Unspecific { sub };
                Some(ClientMapCommand::OnDelAck(sub, source, version))
            }
            _ => None,
        }
    }

    pub fn should_cleanup(&self) -> bool {
        match self {
            MapSlot::Unspecific { .. } => true,
            MapSlot::Remote { value, .. } => value.is_none(),
            MapSlot::Local { value, syncing, .. } => value.is_none() && !*syncing,
        }
    }
}

pub enum LocalMapOutput {
    Remote(ClientMapCommand),
    Local(ServiceId, MapEvent),
}

enum SubState {
    NotSub,
    Subscribing { id: u64, sent_ts: u64 },
    Subscribed { id: u64, remote: NodeSession, sync_ts: u64 },
}

pub struct LocalMap {
    session: NodeSession,
    slots: HashMap<(SubKey, NodeSession), MapSlot>,
    subscribers: Vec<ServiceId>,
    sub_state: SubState,
    queue: VecDeque<LocalMapOutput>,
}

impl LocalMap {
    pub fn new(session: NodeSession) -> Self {
        Self {
            session,
            slots: HashMap::new(),
            subscribers: Vec::new(),
            sub_state: SubState::NotSub,
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        match &mut self.sub_state {
            SubState::NotSub => {}
            SubState::Subscribing { id, sent_ts } => {
                if now >= *sent_ts + RESEND_MS {
                    self.queue.push_back(LocalMapOutput::Remote(ClientMapCommand::Sub(*id, None)));
                    *sent_ts = now;
                }
            }
            SubState::Subscribed { id, remote, sync_ts } => {
                if now >= *sync_ts + SYNC_MS {
                    if self.subscribers.is_empty() {
                        self.queue.push_back(LocalMapOutput::Remote(ClientMapCommand::Unsub(*id)));
                    } else {
                        self.queue.push_back(LocalMapOutput::Remote(ClientMapCommand::Sub(*id, Some(*remote))));
                    }
                    *sync_ts = now;
                }
            }
        }

        for slot in self.slots.values_mut() {
            if let Some(cmd) = slot.sync(now) {
                self.queue.push_back(LocalMapOutput::Remote(cmd));
            }
        }

        // remove all empty slots
        let mut to_remove = vec![];
        for (sub, slot) in self.slots.iter_mut() {
            if slot.should_cleanup() {
                to_remove.push(*sub);
            }
        }

        for sub in to_remove {
            self.slots.remove(&sub);
        }
    }

    pub fn on_control(&mut self, now: u64, service: ServiceId, control: MapControl) -> Option<ClientMapCommand> {
        match control {
            MapControl::Set(sub, data) => {
                let slot = self.get_slot(sub, self.session, true).expect("Must have slot for set");
                slot.set(now, data)
            }
            MapControl::Del(sub) => {
                let slot = self.get_slot(sub, self.session, false)?;
                slot.del(now)
            }
            MapControl::Get => todo!(),
            MapControl::Sub => {
                let send_sub = self.subscribers.is_empty();
                if self.subscribers.contains(&service) {
                    return None;
                }
                self.subscribers.push(service);
                if send_sub {
                    self.sub_state = SubState::Subscribing { sent_ts: now, id: now };
                    Some(ClientMapCommand::Sub(now, None))
                } else {
                    None
                }
            }
            MapControl::Unsub => {
                if !self.subscribers.contains(&service) {
                    return None;
                }
                self.subscribers.retain(|&x| x != service);
                if self.subscribers.is_empty() {
                    Some(ClientMapCommand::Unsub(now))
                } else {
                    None
                }
            }
        }
    }

    /// For OnSet and OnDel event, we need to solve problems: key moved to other server or source server changed.
    /// In case 1 key moved to other server:
    pub fn on_server(&mut self, now: u64, remote: NodeSession, cmd: ServerMapEvent) -> Option<ClientMapCommand> {
        match cmd {
            ServerMapEvent::SetOk(sub, version) => {
                let slot = self.get_slot(sub, self.session, false)?;
                slot.set_ok(version);
                None
            }
            ServerMapEvent::DelOk(sub, version) => {
                let slot = self.get_slot(sub, self.session, false)?;
                slot.del_ok(version);
                None
            }
            ServerMapEvent::SubOk(id) => {
                if let SubState::Subscribing { id: sub_id, .. } = &self.sub_state {
                    if *sub_id == id {
                        self.sub_state = SubState::Subscribed { id, remote, sync_ts: now };
                    }
                }
                None
            }
            ServerMapEvent::UnsubOk(id) => {
                if let SubState::Subscribed { id: sub_id, remote: locked, .. } = &self.sub_state {
                    if *sub_id == id && *locked == remote {
                        self.sub_state = SubState::NotSub;
                    }
                }
                None
            }
            ServerMapEvent::GetOk(_, _) => todo!(),
            ServerMapEvent::OnSet { sub, version, source, data } => {
                if !self.accept_event(remote) {
                    return None;
                }
                let slot = self.get_slot(sub, self.session, true).expect("Must have slot for set");
                let event = slot.on_set(now, sub, source, version, data.clone())?;
                self.fire_event(MapEvent::OnSet(sub, source.0, data));
                Some(event)
            }
            ServerMapEvent::OnDel { sub, version, source } => {
                if !self.accept_event(remote) {
                    return None;
                }

                let slot = self.get_slot(sub, self.session, true).expect("Must have slot for set");
                let event = slot.on_del(now, sub, source, version)?;
                self.fire_event(MapEvent::OnDel(sub, source.0));
                Some(event)
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<LocalMapOutput> {
        self.queue.pop_front()
    }

    pub fn should_cleanup(&self) -> bool {
        self.slots.is_empty() && self.subscribers.is_empty() && matches!(self.sub_state, SubState::NotSub)
    }

    fn get_slot(&mut self, sub: SubKey, source: NodeSession, auto_create: bool) -> Option<&mut MapSlot> {
        if !self.slots.contains_key(&(sub, source)) {
            self.slots.insert((sub, source), MapSlot::new(sub));
        }
        self.slots.get_mut(&(sub, source))
    }

    /// only accept event from locked remote in Subscribed state, otherwise, we will ignore the event.
    fn accept_event(&self, remote: NodeSession) -> bool {
        match &self.sub_state {
            SubState::Subscribed { remote: locked_remote, .. } => *locked_remote == remote,
            _ => false,
        }
    }

    fn fire_event(&mut self, event: MapEvent) {
        for sub in self.subscribers.iter() {
            self.queue.push_back(LocalMapOutput::Local(*sub, event.clone()));
        }
    }
}

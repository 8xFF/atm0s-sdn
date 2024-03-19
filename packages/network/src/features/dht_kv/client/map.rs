use std::collections::{HashMap, VecDeque};

use crate::{
    base::FeatureControlActor,
    features::dht_kv::{
        msg::{ClientMapCommand, NodeSession, ServerMapEvent, Version},
        MapControl, MapEvent, SubKey,
    },
};

const RESEND_MS: u64 = 200; //We will resend set or del command if we don't get ack in this time
const SYNC_MS: u64 = 1500; //We will predict send current state for avoiding out-of-sync, this can be waste of bandwidth but it's better than out-of-sync
const UNSUB_TIMEOUT_MS: u64 = 10000; //We will remove the slot if it's not synced in this time

/// MapSlot manage state of single sub-key inside a map.
enum MapSlot {
    Unspecific {
        sub: SubKey,
    },
    Remote {
        sub: SubKey,
        version: Version,
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

    pub fn data(&self) -> Option<&[u8]> {
        match self {
            MapSlot::Unspecific { .. } => None,
            MapSlot::Remote { value, .. } => value.as_deref(),
            MapSlot::Local { value, .. } => value.as_deref(),
        }
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

    /// We resend the last command if we don't get ack in RESEND_MS, and we predictcaly send current state in SYNC_MS
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
                if (*syncing && now >= *last_sync + RESEND_MS) || now >= *last_sync + SYNC_MS {
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

    pub fn on_set(&mut self, _now: u64, sub: SubKey, source: NodeSession, version: Version, data: Vec<u8>) -> Option<ClientMapCommand> {
        match self {
            MapSlot::Unspecific { .. } => {
                *self = MapSlot::Remote { sub, version, value: Some(data) };
                Some(ClientMapCommand::OnSetAck(sub, source, version))
            }
            MapSlot::Remote { version: old_version, .. } => {
                if old_version.0 <= version.0 {
                    *self = MapSlot::Remote { sub, version, value: Some(data) };
                    Some(ClientMapCommand::OnSetAck(sub, source, version))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn on_del(&mut self, _now: u64, sub: SubKey, source: NodeSession, version: Version) -> Option<ClientMapCommand> {
        match self {
            MapSlot::Remote { version: old_version, .. } => {
                if old_version.0 <= version.0 {
                    *self = MapSlot::Unspecific { sub };
                    Some(ClientMapCommand::OnDelAck(sub, source, version))
                } else {
                    None
                }
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

#[derive(Debug, PartialEq, Eq)]
pub enum LocalMapOutput {
    Remote(ClientMapCommand),
    Local(FeatureControlActor, MapEvent),
}

enum SubState {
    NotSub,
    Subscribing { id: u64, sent_ts: u64 },
    Subscribed { id: u64, remote: NodeSession, sync_ts: u64 },
    Unsubscribing { id: u64, started_at: u64, remote: Option<NodeSession>, sent_ts: u64 },
}

/// LocalMap manage state of map, which is a collection of MapSlot.
/// We allow multi-source for a single sub-key with reason for solving conflict between nodes.
pub struct LocalMap {
    session: NodeSession,
    slots: HashMap<(SubKey, NodeSession), MapSlot>,
    subscribers: Vec<FeatureControlActor>,
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
                    self.queue.push_back(LocalMapOutput::Remote(ClientMapCommand::Sub(*id, Some(*remote))));
                    *sync_ts = now;
                }
            }
            SubState::Unsubscribing { id, started_at, sent_ts, .. } => {
                if now >= *started_at + UNSUB_TIMEOUT_MS {
                    self.sub_state = SubState::NotSub;
                } else if now >= *sent_ts + RESEND_MS {
                    self.queue.push_back(LocalMapOutput::Remote(ClientMapCommand::Unsub(*id)));
                    *sent_ts = now;
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

    pub fn on_control(&mut self, now: u64, actor: FeatureControlActor, control: MapControl) -> Option<ClientMapCommand> {
        match control {
            MapControl::Set(sub, data) => {
                let slot = self.get_slot(sub, self.session, true).expect("Must have slot for set");
                if let Some(out) = slot.set(now, data.clone()) {
                    self.fire_event(MapEvent::OnSet(sub, self.session.0, data));
                    Some(out)
                } else {
                    None
                }
            }
            MapControl::Del(sub) => {
                let slot = self.get_slot(sub, self.session, false)?;
                if let Some(out) = slot.del(now) {
                    self.fire_event(MapEvent::OnDel(sub, self.session.0));
                    Some(out)
                } else {
                    None
                }
            }
            MapControl::Sub => {
                let send_sub = self.subscribers.is_empty();
                if self.subscribers.contains(&actor) {
                    return None;
                }
                self.subscribers.push(actor);
                if send_sub {
                    self.sub_state = SubState::Subscribing { sent_ts: now, id: now };

                    //We need to send all current local data to the new subscriber, because RELAY will not send it to source node.
                    self.restore_events(actor, true);

                    Some(ClientMapCommand::Sub(now, None))
                } else {
                    self.restore_events(actor, false);
                    None
                }
            }
            MapControl::Unsub => {
                if !self.subscribers.contains(&actor) || self.subscribers.is_empty() {
                    return None;
                }
                self.subscribers.retain(|&x| x != actor);
                if self.subscribers.is_empty() {
                    match &self.sub_state {
                        SubState::Subscribed { id, remote, .. } => {
                            let id = *id;
                            self.sub_state = SubState::Unsubscribing {
                                id,
                                remote: Some(*remote),
                                started_at: now,
                                sent_ts: now,
                            };
                            Some(ClientMapCommand::Unsub(id))
                        }
                        SubState::Subscribing { id, .. } => {
                            let id = *id;
                            self.sub_state = SubState::Unsubscribing {
                                id,
                                remote: None,
                                started_at: now,
                                sent_ts: now,
                            };
                            Some(ClientMapCommand::Unsub(id))
                        }
                        _ => panic!("Should in Subscribed or Subscribing state when do unsub"),
                    }
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
                match &self.sub_state {
                    SubState::Subscribing { id: sub_id, .. } | SubState::Subscribed { id: sub_id, .. } => {
                        if *sub_id == id {
                            //update to new remote
                            self.sub_state = SubState::Subscribed { id, remote, sync_ts: now };
                        }
                    }
                    _ => {}
                }
                None
            }
            ServerMapEvent::UnsubOk(id) => {
                if let SubState::Unsubscribing { id: sub_id, remote: locked, .. } = &self.sub_state {
                    if *sub_id == id && (*locked).unwrap_or(remote) == remote {
                        self.sub_state = SubState::NotSub;
                    }
                }
                None
            }
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
        if !self.slots.contains_key(&(sub, source)) && auto_create {
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

    fn restore_events(&mut self, actor: FeatureControlActor, only_local: bool) {
        for ((sub, source), slot) in self.slots.iter() {
            if only_local && self.session != *source {
                continue;
            }
            if let Some(data) = slot.data() {
                self.queue.push_back(LocalMapOutput::Local(actor, MapEvent::OnSet(*sub, source.0, data.to_vec())));
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        base::FeatureControlActor,
        features::dht_kv::{
            client::map::{LocalMapOutput, RESEND_MS, SYNC_MS},
            msg::{ClientMapCommand, NodeSession, ServerMapEvent, SubKey, Version},
            MapControl, MapEvent,
        },
    };

    use super::{LocalMap, MapSlot};

    #[test]
    fn map_slot_set_local() {
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        //we must output set command with new slot, with Version is now_ms
        assert_eq!(slot.set(100, vec![1, 2, 3, 4]), Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4])));

        //we can delete the slot
        assert_eq!(slot.del(200), Some(ClientMapCommand::Del(sub, Version(100))));
    }

    #[test]
    fn map_slot_sync_local() {
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        //we must output set command with new slot, with Version is now_ms
        assert_eq!(slot.set(100, vec![1, 2, 3, 4]), Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4])));

        assert_eq!(slot.sync(101), None);
        assert_eq!(slot.sync(100 + RESEND_MS), Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4])));

        slot.set_ok(Version(100));
        assert_eq!(slot.sync(100 + RESEND_MS * 2), None);

        //after set_ok we only resend with SYNC_MS
        assert_eq!(slot.sync(100 + RESEND_MS + SYNC_MS), Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4])));
    }

    #[test]
    fn map_slot_full_ack() {
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        //we must output set command with new slot, with Version is now_ms
        assert_eq!(slot.set(100, vec![1, 2, 3, 4]), Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4])));
        slot.set_ok(Version(100));
        assert_eq!(slot.sync(100 + RESEND_MS), None);

        //we must output del command with new slot, with Version is now_ms
        assert_eq!(slot.del(200), Some(ClientMapCommand::Del(sub, Version(100))));
        slot.del_ok(Version(100));
        assert_eq!(slot.sync(200 + RESEND_MS), None);
    }

    #[test]
    fn map_slot_disconnect_ack() {
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        //we must output set command with new slot, with Version is now_ms
        assert_eq!(slot.set(100, vec![1, 2, 3, 4]), Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4])));

        //we missing set_ok, but we delete now
        //we must output del command with new slot, with Version is now_ms
        assert_eq!(slot.del(200), Some(ClientMapCommand::Del(sub, Version(100))));
        slot.del_ok(Version(100));
        assert_eq!(slot.sync(200 + RESEND_MS), None);
    }

    #[test]
    fn map_slot_wrong_ack() {
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        //we must output set command with new slot, with Version is now_ms
        assert_eq!(slot.set(100, vec![1, 2, 3, 4]), Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4])));
        slot.set_ok(Version(101));
        assert_ne!(slot.sync(100 + RESEND_MS), None);

        assert_eq!(slot.del(200 + RESEND_MS), Some(ClientMapCommand::Del(sub, Version(100))));
        slot.del_ok(Version(101));
        assert_ne!(slot.sync(200 + RESEND_MS + RESEND_MS), None);
    }

    #[test]
    fn map_slot_handle_server_event() {
        let source = NodeSession(1, 2);
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        let version = Version(100);
        assert_eq!(slot.on_set(100, sub, source, version, vec![1, 2, 3, 4]), Some(ClientMapCommand::OnSetAck(sub, source, version)));
        assert!(!slot.should_cleanup());

        assert_eq!(slot.on_del(200, sub, source, version), Some(ClientMapCommand::OnDelAck(sub, source, version)));
        assert!(slot.should_cleanup());
    }

    #[test]
    fn map_slot_should_reject_server_event_with_local() {
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        let version = Version(100);
        assert_eq!(slot.set(100, vec![1, 2, 3, 4]), Some(ClientMapCommand::Set(sub, version, vec![1, 2, 3, 4])));

        let source = NodeSession(1, 2);
        assert_eq!(slot.on_set(100, sub, source, version, vec![1, 2, 3, 4]), None);
        assert_eq!(slot.on_del(200, sub, source, version), None);
        assert!(!slot.should_cleanup());
    }

    #[test]
    fn map_slot_should_reject_server_event_without_newer_version() {
        let sub = SubKey(1);
        let mut slot = MapSlot::new(sub);

        let source = NodeSession(1, 2);
        assert_eq!(
            slot.on_set(100, sub, source, Version(100), vec![1, 2, 3, 4]),
            Some(ClientMapCommand::OnSetAck(sub, source, Version(100)))
        );
        assert_eq!(slot.on_set(100, sub, source, Version(90), vec![1, 2, 3]), None);
        assert_eq!(slot.on_del(200, sub, source, Version(90)), None);
        assert!(!slot.should_cleanup());
    }

    #[test]
    fn map_handle_control_set_del_correct() {
        let session = NodeSession(1, 2);
        let actor = FeatureControlActor::Controller;
        let mut map = LocalMap::new(session);

        let sub = SubKey(1);

        assert_eq!(
            map.on_control(100, actor, MapControl::Set(sub, vec![1, 2, 3, 4])),
            Some(ClientMapCommand::Set(sub, Version(100), vec![1, 2, 3, 4]))
        );
        assert_eq!(map.on_control(102, actor, MapControl::Sub), Some(ClientMapCommand::Sub(102, None))); //Sub will be sent include time ms
        assert_eq!(map.pop_action(), Some(LocalMapOutput::Local(actor, MapEvent::OnSet(sub, session.0, vec![1, 2, 3, 4]))));
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_control(101, actor, MapControl::Del(sub)), Some(ClientMapCommand::Del(sub, Version(100))));
        assert_eq!(map.pop_action(), Some(LocalMapOutput::Local(actor, MapEvent::OnDel(sub, session.0))));
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_handle_sub_with_local_data_correct() {
        let session = NodeSession(1, 2);
        let actor = FeatureControlActor::Controller;
        let mut map = LocalMap::new(session);

        let sub = SubKey(1);

        assert_eq!(map.on_control(102, actor, MapControl::Sub), Some(ClientMapCommand::Sub(102, None)));
        assert_eq!(
            map.on_control(103, actor, MapControl::Set(sub, vec![1, 2, 3, 4])),
            Some(ClientMapCommand::Set(sub, Version(103), vec![1, 2, 3, 4]))
        );
        assert_eq!(map.pop_action(), Some(LocalMapOutput::Local(actor, MapEvent::OnSet(sub, session.0, vec![1, 2, 3, 4]))));
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_control(103, actor, MapControl::Unsub), Some(ClientMapCommand::Unsub(102)));
        assert_eq!(map.on_control(104, actor, MapControl::Del(sub)), Some(ClientMapCommand::Del(sub, Version(103))));
        assert_eq!(map.pop_action(), None); //should not output local event after unsub
    }

    #[test]
    fn map_unsub_ok() {
        let session = NodeSession(1, 2);
        let actor = FeatureControlActor::Controller;
        let mut map = LocalMap::new(session);

        let relay = NodeSession(5, 6);

        assert_eq!(map.on_control(102, actor, MapControl::Sub), Some(ClientMapCommand::Sub(102, None)));
        assert_eq!(map.on_server(103, relay, ServerMapEvent::SubOk(102)), None);

        map.on_tick(102 + RESEND_MS);
        assert_eq!(map.pop_action(), None);

        assert_eq!(map.on_control(103, actor, MapControl::Unsub), Some(ClientMapCommand::Unsub(102)));
        assert_eq!(map.on_server(104, relay, ServerMapEvent::UnsubOk(102)), None);
        map.on_tick(103 + RESEND_MS);
        assert_eq!(map.pop_action(), None);

        assert!(map.should_cleanup());
    }

    #[test]
    fn map_handle_auto_resend_sub_unsub() {
        let session = NodeSession(1, 2);
        let actor = FeatureControlActor::Controller;
        let mut map = LocalMap::new(session);

        assert_eq!(map.on_control(102, actor, MapControl::Sub), Some(ClientMapCommand::Sub(102, None)));
        map.on_tick(102 + RESEND_MS);
        assert_eq!(map.pop_action(), Some(LocalMapOutput::Remote(ClientMapCommand::Sub(102, None))));
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_handle_set_del_ack() {
        let session = NodeSession(1, 2);
        let actor = FeatureControlActor::Controller;
        let mut map = LocalMap::new(session);

        let relay = NodeSession(3, 4);

        assert_eq!(map.on_control(102, actor, MapControl::Sub), Some(ClientMapCommand::Sub(102, None)));
        assert_eq!(map.on_server(103, relay, ServerMapEvent::SubOk(102)), None);
        map.on_tick(102 + RESEND_MS);
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_reject_event_unknown_relay() {
        let session = NodeSession(1, 2);
        let mut map = LocalMap::new(session);

        let sub = SubKey(1);
        let source = NodeSession(3, 4);
        let relay = NodeSession(3, 4);
        assert_eq!(
            map.on_server(
                103,
                relay,
                ServerMapEvent::OnSet {
                    sub,
                    source,
                    version: Version(2000),
                    data: vec![1, 2, 3, 4]
                }
            ),
            None
        );
    }

    #[test]
    fn map_handle_sub_with_remote_data_correct() {
        let session = NodeSession(1, 2);
        let actor = FeatureControlActor::Controller;
        let mut map = LocalMap::new(session);

        let sub = SubKey(1);

        let source = NodeSession(3, 4);
        let relay = NodeSession(5, 6);
        assert_eq!(map.on_control(102, actor, MapControl::Sub), Some(ClientMapCommand::Sub(102, None)));

        //We need SubOk for accepting OnSet event
        assert_eq!(map.on_server(103, relay, ServerMapEvent::SubOk(102)), None);

        assert_eq!(
            map.on_server(
                103,
                relay,
                ServerMapEvent::OnSet {
                    sub,
                    source,
                    version: Version(2000),
                    data: vec![1, 2, 3, 4]
                }
            ),
            Some(ClientMapCommand::OnSetAck(sub, source, Version(2000)))
        );
        assert_eq!(map.pop_action(), Some(LocalMapOutput::Local(actor, MapEvent::OnSet(sub, source.0, vec![1, 2, 3, 4]))));
        assert_eq!(map.pop_action(), None);

        assert_eq!(
            map.on_server(103, relay, ServerMapEvent::OnDel { sub, source, version: Version(2000) }),
            Some(ClientMapCommand::OnDelAck(sub, source, Version(2000)))
        );
        assert_eq!(map.pop_action(), Some(LocalMapOutput::Local(actor, MapEvent::OnDel(sub, source.0))));
        assert_eq!(map.pop_action(), None);
    }

    #[test]
    fn map_handle_switch_new_relay() {
        let session = NodeSession(1, 2);
        let actor = FeatureControlActor::Controller;
        let mut map = LocalMap::new(session);

        let sub = SubKey(1);
        let source = NodeSession(3, 4);
        let relay1 = NodeSession(5, 6);
        let relay2 = NodeSession(7, 8);

        assert_eq!(map.on_control(102, actor, MapControl::Sub), Some(ClientMapCommand::Sub(102, None)));
        assert_eq!(map.on_server(103, relay1, ServerMapEvent::SubOk(102)), None);

        assert_eq!(
            map.on_server(
                103,
                relay1,
                ServerMapEvent::OnSet {
                    sub,
                    source,
                    version: Version(2000),
                    data: vec![1, 2, 3, 4]
                }
            ),
            Some(ClientMapCommand::OnSetAck(sub, source, Version(2000)))
        );

        //simulate sub to new relay
        assert_eq!(map.on_server(105, relay2, ServerMapEvent::SubOk(102)), None);

        //Reject from relay1
        assert_eq!(map.on_server(106, relay1, ServerMapEvent::OnDel { sub, source, version: Version(2000) }), None,);

        //from relay2
        assert_eq!(
            map.on_server(
                107,
                relay2,
                ServerMapEvent::OnSet {
                    sub,
                    source,
                    version: Version(2000),
                    data: vec![1, 2, 3, 4]
                }
            ),
            Some(ClientMapCommand::OnSetAck(sub, source, Version(2000)))
        );
    }
}

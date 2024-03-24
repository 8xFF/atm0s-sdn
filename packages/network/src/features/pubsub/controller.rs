use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::NodeId;

use crate::base::{ConnectionEvent, Feature, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput};

use super::{
    msg::{ChannelId, RelayControl, RelayId},
    ChannelControl, ChannelEvent, Control, Event, RelayWorkerControl, ToController, ToWorker, FEATURE_ID, FEATURE_NAME,
};

const RELAY_TIMEOUT: u64 = 10_000;

enum RelayState {
    New,
    Binding {
        remotes: HashMap<SocketAddr, RelayRemote>,
        locals: Vec<FeatureControlActor>,
    },
    Bound {
        next: SocketAddr,
        remotes: HashMap<SocketAddr, RelayRemote>,
        locals: Vec<FeatureControlActor>,
    },
    Unbinding {
        next: SocketAddr,
        started_at: u64,
    },
    Unbound,
}

struct RelayRemote {
    uuid: u64,
    last_sub: u64,
}

pub struct Relay {
    uuid: u64,
    state: RelayState,
    queue: VecDeque<RelayWorkerControl>,
}

impl Relay {
    pub fn new(uuid: u64) -> Self {
        Self {
            uuid,
            state: RelayState::New,
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        match &mut self.state {
            RelayState::Bound { next, locals, remotes, .. } => {
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, Some(*next)));
                let mut timeout = vec![];
                for (remote, slot) in remotes.iter() {
                    if now >= slot.last_sub + RELAY_TIMEOUT {
                        timeout.push(*remote);
                    }
                }

                for remote in timeout {
                    self.queue.push_back(RelayWorkerControl::RouteDelRemote(remote));
                    remotes.remove(&remote);
                }

                if remotes.is_empty() && locals.is_empty() {
                    self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                    self.state = RelayState::Unbinding { next: *next, started_at: now };
                }
            }
            RelayState::Binding { remotes, locals, .. } => {
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
                let mut timeout = vec![];
                for (remote, slot) in remotes.iter() {
                    if now >= slot.last_sub + RELAY_TIMEOUT {
                        timeout.push(*remote);
                    }
                }

                for remote in timeout {
                    self.queue.push_back(RelayWorkerControl::RouteDelRemote(remote));
                    remotes.remove(&remote);
                }

                if remotes.is_empty() && locals.is_empty() {
                    self.state = RelayState::Unbound;
                }
            }
            RelayState::Unbinding { next, started_at } => {
                if now >= *started_at + RELAY_TIMEOUT {
                    self.state = RelayState::Unbound;
                } else {
                    self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                }
            }
            _ => {}
        }
    }

    pub fn conn_disconnected(&mut self, _now: u64, remote: SocketAddr) {
        match &mut self.state {
            RelayState::Bound { next, remotes, locals, .. } => {
                if *next == remote {
                    self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
                    let remotes = std::mem::replace(remotes, HashMap::new());
                    let locals = std::mem::replace(locals, vec![]);
                    self.state = RelayState::Binding { remotes, locals };
                }
            }
            _ => {}
        }
    }

    /// Add a local subscriber to the relay
    /// Returns true if this is the first subscriber, false otherwise
    pub fn on_local_sub(&mut self, _now: u64, actor: FeatureControlActor) {
        match &mut self.state {
            RelayState::New | RelayState::Unbound => {
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
                self.state = RelayState::Binding {
                    remotes: HashMap::new(),
                    locals: vec![actor],
                };
            }
            RelayState::Unbinding { next, .. } => {
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, Some(*next)));
                self.state = RelayState::Binding {
                    remotes: HashMap::new(),
                    locals: vec![actor],
                };
            }
            RelayState::Binding { locals, .. } => {
                locals.push(actor);
            }
            RelayState::Bound { locals, .. } => {
                locals.push(actor);
            }
        }

        self.queue.push_back(RelayWorkerControl::RouteSetLocal(actor));
    }

    /// Remove a local subscriber from the relay
    /// Returns true if this is the last subscriber, false otherwise
    pub fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor) {
        match &mut self.state {
            RelayState::New | RelayState::Unbound => {
                log::warn!("[Relay] Unsub for unknown relay {:?}", self.uuid);
            }
            RelayState::Unbinding { .. } => {
                log::warn!("[Relay] Unsub for relay in unbinding state {:?}", self.uuid);
            }
            RelayState::Binding { locals, remotes, .. } => {
                if let Some(pos) = locals.iter().position(|a| *a == actor) {
                    self.queue.push_back(RelayWorkerControl::RouteDelLocal(actor));
                    locals.swap_remove(pos);
                    if locals.is_empty() && remotes.is_empty() {
                        self.state = RelayState::Unbound;
                    }
                } else {
                    log::warn!("[Relay] Unsub for unknown local actor {:?}", actor);
                }
            }
            RelayState::Bound { next, locals, remotes, .. } => {
                if let Some(pos) = locals.iter().position(|a| *a == actor) {
                    self.queue.push_back(RelayWorkerControl::RouteDelLocal(actor));
                    locals.swap_remove(pos);
                    if locals.is_empty() && remotes.is_empty() {
                        self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                        self.state = RelayState::Unbinding { next: *next, started_at: now };
                    }
                } else {
                    log::warn!("[Relay] Unsub for unknown local actor {:?}", actor);
                }
            }
        }
    }

    pub fn on_remote(&mut self, now: u64, remote: SocketAddr, control: RelayControl) {
        match control {
            RelayControl::Sub(uuid) => {
                match &mut self.state {
                    RelayState::New | RelayState::Unbound => {
                        self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
                        self.state = RelayState::Binding {
                            remotes: HashMap::from([(remote, RelayRemote { uuid, last_sub: now })]),
                            locals: vec![],
                        };
                    }
                    RelayState::Unbinding { next, .. } => {
                        self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, Some(*next)));
                        self.state = RelayState::Binding {
                            remotes: HashMap::from([(remote, RelayRemote { uuid, last_sub: now })]),
                            locals: vec![],
                        };
                    }
                    RelayState::Binding { remotes, .. } | RelayState::Bound { remotes, .. } => {
                        remotes.insert(remote, RelayRemote { uuid, last_sub: now });
                    }
                }
                self.queue.push_back(RelayWorkerControl::RouteSetRemote(remote));
            }
            RelayControl::Unsub(uuid) => match &mut self.state {
                RelayState::New | RelayState::Unbound => {
                    log::warn!("[Relay] Unsub for unknown relay {:?}", self.uuid);
                }
                RelayState::Unbinding { .. } => {
                    log::warn!("[Relay] Unsub for relay in unbinding state {:?}", self.uuid);
                }
                RelayState::Binding { remotes, locals, .. } => {
                    if let Some(slot) = remotes.get(&remote) {
                        if slot.uuid == uuid {
                            self.queue.push_back(RelayWorkerControl::RouteDelRemote(remote));
                            remotes.remove(&remote);
                            if remotes.is_empty() && locals.is_empty() {
                                self.state = RelayState::Unbound;
                            }
                        } else {
                            log::warn!("[Relay] Unsub for wrong session remote {remote}, {uuid} vs {}", slot.uuid);
                        }
                    } else {
                        log::warn!("[Relay] Unsub for unknown remote {:?}", remote);
                    }
                }
                RelayState::Bound { next, remotes, locals } => {
                    if let Some(slot) = remotes.get(&remote) {
                        if slot.uuid == uuid {
                            self.queue.push_back(RelayWorkerControl::RouteDelRemote(remote));
                            remotes.remove(&remote);
                            if remotes.is_empty() && locals.is_empty() {
                                self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                                self.state = RelayState::Unbinding { next: *next, started_at: now };
                            }
                        } else {
                            log::warn!("[Relay] Unsub for wrong session remote {remote}, {uuid} vs {}", slot.uuid);
                        }
                    } else {
                        log::warn!("[Relay] Unsub for unknown remote {:?}", remote);
                    }
                }
            },
            RelayControl::SubOK(uuid) => {
                if uuid != self.uuid {
                    log::warn!("[Relay] SubOK for wrong relay session {uuid} vs {}", uuid);
                }
                match &mut self.state {
                    RelayState::Binding { remotes, locals } => {
                        let remotes = std::mem::replace(remotes, HashMap::new());
                        let locals = std::mem::replace(locals, vec![]);
                        self.state = RelayState::Bound { next: remote, remotes, locals };
                    }
                    _ => {}
                }
            }
            RelayControl::UnsubOK(uuid) => {
                if uuid != self.uuid {
                    log::warn!("[Relay] UnsubOK for wrong relay session {uuid} vs {}", uuid);
                }

                match &mut self.state {
                    RelayState::Unbinding { .. } => {
                        self.state = RelayState::Unbound;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn pop_output(&mut self) -> Option<RelayWorkerControl> {
        self.queue.pop_front()
    }

    pub fn relay_dests(&self) -> Option<(&[FeatureControlActor], bool)> {
        match &self.state {
            RelayState::Bound { locals, remotes, .. } | RelayState::Binding { locals, remotes, .. } => Some((locals.as_slice(), !remotes.is_empty())),
            _ => None,
        }
    }

    pub fn should_clear(&self) -> bool {
        matches!(self.state, RelayState::Unbound)
    }
}

pub struct PubSubFeature {
    node_id: NodeId,
    session: u64,
    relays: HashMap<RelayId, Relay>,
    queue: VecDeque<FeatureOutput<Event, ToWorker>>,
}

impl PubSubFeature {
    pub fn new(node_id: NodeId, session: u64) -> Self {
        Self {
            node_id,
            session,
            relays: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    fn get_relay(&mut self, relay_id: RelayId, auto_create: bool) -> Option<&mut Relay> {
        if !self.relays.contains_key(&relay_id) && auto_create {
            log::info!("[PubSubFeatureController] Creating new relay: {:?}", relay_id);
            self.relays.insert(relay_id, Relay::new(self.session));
        }
        self.relays.get_mut(&relay_id)
    }

    fn on_local(&mut self, now: u64, actor: FeatureControlActor, channel: ChannelId, control: ChannelControl) {
        match control {
            ChannelControl::SubSource(source) => {
                let relay_id = RelayId(channel, source);
                let relay = self.get_relay(relay_id, true).expect("Should create");
                log::debug!("[PubSubFeatureController] Sub for {:?} from {:?}", relay_id, actor);
                relay.on_local_sub(now, actor);
                Self::pop_single_relay(relay_id, self.relays.get_mut(&relay_id).expect("Should have"), &mut self.queue);
            }
            ChannelControl::UnsubSource(source) => {
                let relay_id = RelayId(channel, source);
                if let Some(relay) = self.get_relay(relay_id, false) {
                    log::debug!("[PubSubFeatureController] Unsub for {:?} from {:?}", relay_id, actor);
                    relay.on_local_unsub(now, actor);
                    Self::pop_single_relay(relay_id, self.relays.get_mut(&relay_id).expect("Should have"), &mut self.queue);
                } else {
                    log::warn!("[PubSubFeatureController] Unsub for unknown relay {:?}", relay_id);
                }
            }
            ChannelControl::PubData(data) => {
                let relay_id = RelayId(channel, self.node_id);
                if let Some(relay) = self.relays.get(&relay_id) {
                    if let Some((locals, has_remote)) = relay.relay_dests() {
                        for local in locals {
                            self.queue.push_back(FeatureOutput::Event(*local, Event(channel, ChannelEvent::SourceData(self.node_id, data.clone()))));
                        }

                        if has_remote {
                            self.queue.push_back(FeatureOutput::ToWorker(true, ToWorker::RelayData(relay_id, data)));
                        }
                    }
                }
            }
        }
    }

    fn on_remote(&mut self, now: u64, remote: SocketAddr, relay_id: RelayId, control: RelayControl) {
        if let Some(relay) = self.get_relay(relay_id, control.should_create()) {
            log::debug!("[PubSubFeatureController] Remote control for {:?} from {:?}: {:?}", relay_id, remote, control);
            relay.on_remote(now, remote, control);
            Self::pop_single_relay(relay_id, self.relays.get_mut(&relay_id).expect("Should have relay"), &mut self.queue);
        } else {
            log::warn!("[PubSubFeatureController] Remote control for unknown relay {:?}", relay_id);
        }
    }

    fn pop_single_relay(relay_id: RelayId, relay: &mut Relay, queue: &mut VecDeque<FeatureOutput<Event, ToWorker>>) {
        while let Some(control) = relay.pop_output() {
            queue.push_back(FeatureOutput::ToWorker(control.is_broadcast(), ToWorker::RelayWorkerControl(relay_id, control)));
        }
    }
}

impl Feature<Control, Event, ToController, ToWorker> for PubSubFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Tick(_) => {
                let mut clears = vec![];
                for (relay_id, relay) in self.relays.iter_mut() {
                    if relay.should_clear() {
                        clears.push(*relay_id);
                    } else {
                        relay.on_tick(now);
                        Self::pop_single_relay(*relay_id, relay, &mut self.queue);
                    }
                }
                for relay_id in clears {
                    self.relays.remove(&relay_id);
                }
            }
            FeatureSharedInput::Connection(event) => match event {
                ConnectionEvent::Disconnected(ctx) => {
                    for (relay_id, relay) in self.relays.iter_mut() {
                        relay.conn_disconnected(now, ctx.remote);
                        Self::pop_single_relay(*relay_id, relay, &mut self.queue);
                    }
                }
                _ => {}
            },
        }
    }

    fn on_input<'a>(&mut self, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::FromWorker(ToController::RemoteControl(remote, relay_id, control)) => {
                self.on_remote(_now_ms, remote, relay_id, control);
            }
            FeatureInput::Control(actor, Control(channel, control)) => {
                self.on_local(_now_ms, actor, channel, control);
            }
            _ => panic!("Unexpected input"),
        }
    }

    fn pop_output<'a>(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        self.queue.pop_front()
    }
}

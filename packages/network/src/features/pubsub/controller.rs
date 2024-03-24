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

pub const RELAY_TIMEOUT: u64 = 10_000;

mod consumers;
mod local_relay;
mod remote_relay;

use local_relay::LocalRelay;
use remote_relay::RemoteRelay;

pub trait GenericRelay {
    fn on_tick(&mut self, now: u64);
    fn on_local_sub(&mut self, now: u64, actor: FeatureControlActor);
    fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor);
    fn on_remote(&mut self, now: u64, remote: SocketAddr, control: RelayControl);
    fn conn_disconnected(&mut self, now: u64, remote: SocketAddr);
    fn should_clear(&self) -> bool;
    fn relay_dests(&self) -> Option<(&[FeatureControlActor], bool)>;
    fn pop_output(&mut self) -> Option<RelayWorkerControl>;
}

pub struct PubSubFeature {
    node_id: NodeId,
    session: u64,
    relays: HashMap<RelayId, Box<dyn GenericRelay>>,
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

    fn get_relay(&mut self, relay_id: RelayId, auto_create: bool) -> Option<&mut Box<dyn GenericRelay>> {
        if !self.relays.contains_key(&relay_id) && auto_create {
            log::info!("[PubSubFeatureController] Creating new relay: {:?}", relay_id);
            let relay: Box<dyn GenericRelay> = if self.node_id == relay_id.1 {
                Box::new(LocalRelay::default())
            } else {
                Box::new(RemoteRelay::new(self.session))
            };
            self.relays.insert(relay_id, relay);
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

    fn pop_single_relay(relay_id: RelayId, relay: &mut Box<dyn GenericRelay>, queue: &mut VecDeque<FeatureOutput<Event, ToWorker>>) {
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

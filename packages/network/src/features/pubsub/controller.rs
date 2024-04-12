use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use crate::base::{ConnectionEvent, Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput};

use self::source_hint::SourceHintLogic;

use super::{
    msg::{ChannelId, RelayControl, RelayId, SourceHint},
    ChannelControl, ChannelEvent, Control, Event, RelayWorkerControl, ToController, ToWorker,
};

pub const RELAY_TIMEOUT: u64 = 10_000;
pub const RELAY_STICKY_MS: u64 = 5 * 60 * 1000; //sticky route path in 5 minutes

mod consumers;
mod local_relay;
mod remote_relay;
mod source_hint;

use atm0s_sdn_identity::NodeId;
use local_relay::LocalRelay;
use remote_relay::RemoteRelay;

#[derive(Debug, PartialEq, Eq)]
pub enum GenericRelayOutput {
    ToWorker(RelayWorkerControl),
    RouteChanged(FeatureControlActor),
}

pub trait GenericRelay {
    fn on_tick(&mut self, now: u64);
    fn on_local_sub(&mut self, now: u64, actor: FeatureControlActor);
    fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor);
    fn on_remote(&mut self, now: u64, remote: SocketAddr, control: RelayControl);
    fn conn_disconnected(&mut self, now: u64, remote: SocketAddr);
    fn should_clear(&self) -> bool;
    fn relay_dests(&self) -> Option<(&[FeatureControlActor], bool)>;
    fn pop_output(&mut self) -> Option<GenericRelayOutput>;
}

pub struct PubSubFeature {
    relays: HashMap<RelayId, Box<dyn GenericRelay>>,
    source_hints: HashMap<ChannelId, SourceHintLogic>,
    queue: VecDeque<FeatureOutput<Event, ToWorker>>,
}

impl PubSubFeature {
    pub fn new() -> Self {
        Self {
            relays: HashMap::new(),
            source_hints: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    fn get_relay(&mut self, ctx: &FeatureContext, relay_id: RelayId, auto_create: bool) -> Option<&mut Box<dyn GenericRelay>> {
        if !self.relays.contains_key(&relay_id) && auto_create {
            let relay: Box<dyn GenericRelay> = if ctx.node_id == relay_id.1 {
                log::info!("[PubSubFeatureController] Creating new LocalRelay: {}", relay_id.0);
                Box::new(LocalRelay::default())
            } else {
                log::info!("[PubSubFeatureController] Creating new RemoteController: {:?}", relay_id);
                Box::new(RemoteRelay::new(ctx.session))
            };
            self.relays.insert(relay_id, relay);
        }
        self.relays.get_mut(&relay_id)
    }

    fn get_source_hint(&mut self, node_id: NodeId, session: u64, channel: ChannelId, auto_create: bool) -> Option<&mut SourceHintLogic> {
        if !self.source_hints.contains_key(&channel) && auto_create {
            self.source_hints.insert(channel, SourceHintLogic::new(node_id, session));
        }
        self.source_hints.get_mut(&channel)
    }

    fn on_local(&mut self, ctx: &FeatureContext, now: u64, actor: FeatureControlActor, channel: ChannelId, control: ChannelControl) {
        match control {
            ChannelControl::SubAuto => {
                let sh = self.get_source_hint(ctx.node_id, ctx.session, channel, true).expect("Should create");
                sh.on_local(now, actor, source_hint::LocalCmd::Subscribe);
                self.pop_single_source_hint(ctx, now, channel);
            }
            ChannelControl::UnsubAuto => {
                if let Some(sh) = self.get_source_hint(ctx.node_id, ctx.session, channel, false) {
                    sh.on_local(now, actor, source_hint::LocalCmd::Unsubscribe);
                    self.pop_single_source_hint(ctx, now, channel);
                }
            }
            ChannelControl::PubStart => {
                let sh = self.get_source_hint(ctx.node_id, ctx.session, channel, true).expect("Should create");
                sh.on_local(now, actor, source_hint::LocalCmd::Register);
                self.pop_single_source_hint(ctx, now, channel);
            }
            ChannelControl::PubStop => {
                if let Some(sh) = self.get_source_hint(ctx.node_id, ctx.session, channel, false) {
                    sh.on_local(now, actor, source_hint::LocalCmd::Unregister);
                    self.pop_single_source_hint(ctx, now, channel);
                }
            }
            ChannelControl::SubSource(source) => {
                let relay_id = RelayId(channel, source);
                let relay = self.get_relay(ctx, relay_id, true).expect("Should create");
                log::debug!("[PubSubFeatureController] Sub for {:?} from {:?}", relay_id, actor);
                relay.on_local_sub(now, actor);
                Self::pop_single_relay(relay_id, self.relays.get_mut(&relay_id).expect("Should have"), &mut self.queue);
            }
            ChannelControl::UnsubSource(source) => {
                let relay_id = RelayId(channel, source);
                if let Some(relay) = self.relays.get_mut(&relay_id) {
                    log::debug!("[PubSubFeatureController] Unsub for {:?} from {:?}", relay_id, actor);
                    relay.on_local_unsub(now, actor);
                    Self::pop_single_relay(relay_id, relay, &mut self.queue);
                    if relay.should_clear() {
                        self.relays.remove(&relay_id);
                    }
                } else {
                    log::warn!("[PubSubFeatureController] Unsub for unknown relay {:?}", relay_id);
                }
            }
            ChannelControl::PubData(data) => {
                let relay_id = RelayId(channel, ctx.node_id);
                if let Some(relay) = self.relays.get(&relay_id) {
                    if let Some((locals, has_remote)) = relay.relay_dests() {
                        log::debug!(
                            "[PubSubFeatureController] Pub for {:?} from {:?} to {:?} locals, has remote {has_remote}",
                            relay_id,
                            actor,
                            locals.len()
                        );
                        for local in locals {
                            self.queue.push_back(FeatureOutput::Event(*local, Event(channel, ChannelEvent::SourceData(ctx.node_id, data.clone()))));
                        }

                        if has_remote {
                            self.queue.push_back(FeatureOutput::ToWorker(true, ToWorker::RelayData(relay_id, data)));
                        }
                    } else {
                        log::debug!("[PubSubFeatureController] No subscribers for {:?}, dropping data from {:?}", relay_id, actor)
                    }
                } else {
                    log::warn!("[PubSubFeatureController] Pub for unknown relay {:?}", relay_id);
                }
            }
        }
    }

    fn on_remote_relay_control(&mut self, ctx: &FeatureContext, now: u64, remote: SocketAddr, relay_id: RelayId, control: RelayControl) {
        if let Some(_) = self.get_relay(ctx, relay_id, control.should_create()) {
            let relay: &mut Box<dyn GenericRelay> = self.relays.get_mut(&relay_id).expect("Should have relay");
            log::debug!("[PubSubFeatureController] Remote control for {:?} from {:?}: {:?}", relay_id, remote, control);
            relay.on_remote(now, remote, control);
            Self::pop_single_relay(relay_id, relay, &mut self.queue);
            if relay.should_clear() {
                self.relays.remove(&relay_id);
            }
        } else {
            log::warn!("[PubSubFeatureController] Remote control for unknown relay {:?}", relay_id);
        }
    }

    fn on_remote_source_hint_control(&mut self, ctx: &FeatureContext, now: u64, remote: SocketAddr, channel: ChannelId, control: SourceHint) {
        if let Some(sh) = self.get_source_hint(ctx.node_id, ctx.session, channel, control.should_create()) {
            log::debug!("[PubSubFeatureController] SourceHint control for {:?} from {:?}: {:?}", channel, remote, control);
            sh.on_remote(now, remote, control);
            self.pop_single_source_hint(ctx, now, channel);

            let sh = self.source_hints.get_mut(&channel).expect("Should have source hint");
            if sh.should_clear() {
                self.source_hints.remove(&channel);
            }
        } else {
            log::warn!("[PubSubFeatureController] Remote control for unknown channel {:?}", channel);
        }
    }

    fn pop_single_relay(relay_id: RelayId, relay: &mut Box<dyn GenericRelay>, queue: &mut VecDeque<FeatureOutput<Event, ToWorker>>) {
        while let Some(control) = relay.pop_output() {
            queue.push_back(match control {
                GenericRelayOutput::ToWorker(control) => FeatureOutput::ToWorker(true, ToWorker::RelayControl(relay_id, control)),
                GenericRelayOutput::RouteChanged(actor) => FeatureOutput::Event(actor, Event(relay_id.0, ChannelEvent::RouteChanged(relay_id.1))),
            });
        }
    }

    fn pop_single_source_hint(&mut self, ctx: &FeatureContext, now: u64, channel: ChannelId) {
        loop {
            let sh = self.source_hints.get_mut(&channel).expect("Should have source hint");
            let out = if let Some(out) = sh.pop_output() {
                out
            } else {
                return;
            };
            match out {
                source_hint::Output::SendRemote(dest, control) => {
                    self.queue.push_back(FeatureOutput::ToWorker(true, ToWorker::SourceHint(channel, dest, control)));
                }
                source_hint::Output::SubscribeSource(actors, source) => {
                    for actor in actors {
                        self.on_local(ctx, now, actor, channel, ChannelControl::SubSource(source));
                    }
                }
                source_hint::Output::UnsubscribeSource(actors, source) => {
                    for actor in actors {
                        self.on_local(ctx, now, actor, channel, ChannelControl::UnsubSource(source));
                    }
                }
            }
        }
    }
}

impl Feature<Control, Event, ToController, ToWorker> for PubSubFeature {
    fn on_shared_input(&mut self, ctx: &FeatureContext, now: u64, input: FeatureSharedInput) {
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

                let mut clears = vec![];
                let mut not_clears = vec![];
                for (channel, sh) in self.source_hints.iter_mut() {
                    if sh.should_clear() {
                        clears.push(*channel);
                    } else {
                        sh.on_tick(now);
                        not_clears.push(*channel);
                    }
                }
                for channel in clears {
                    self.source_hints.remove(&channel);
                }
                for channel in not_clears {
                    self.pop_single_source_hint(ctx, now, channel);
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

    fn on_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::FromWorker(ToController::RelayControl(remote, relay_id, control)) => {
                self.on_remote_relay_control(ctx, now_ms, remote, relay_id, control);
            }
            FeatureInput::FromWorker(ToController::SourceHint(remote, channel, control)) => {
                self.on_remote_source_hint_control(ctx, now_ms, remote, channel, control);
            }
            FeatureInput::Control(actor, Control(channel, control)) => {
                self.on_local(ctx, now_ms, actor, channel, control);
            }
            _ => panic!("Unexpected input"),
        }
    }

    fn pop_output<'a>(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<Event, ToWorker>> {
        self.queue.pop_front()
    }
}

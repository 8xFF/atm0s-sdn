use std::{collections::HashMap, fmt::Debug};

use atm0s_sdn_identity::ConnId;
use atm0s_sdn_router::{RouteAction, RouterTable};
use sans_io_runtime::{collections::DynamicDeque, return_if_err, return_if_none, TaskSwitcherChild};

use crate::{
    base::{Buffer, FeatureControlActor, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, TransportMsgHeader},
    data_plane::NetPair,
};

use super::{
    msg::{PubsubMessage, RelayControl, RelayId},
    ChannelControl, ChannelEvent, Control, Event, RelayWorkerControl, ToController, ToWorker,
};

struct WorkerRelay<UserData> {
    source: Option<NetPair>,
    locals: Vec<FeatureControlActor<UserData>>,
    remotes: Vec<NetPair>,
    remotes_uuid: HashMap<NetPair, u64>,
}

impl<UserData> WorkerRelay<UserData> {
    pub fn is_empty(&self) -> bool {
        self.locals.is_empty() && self.remotes.is_empty()
    }
}

pub struct PubSubFeatureWorker<UserData> {
    relays: HashMap<RelayId, WorkerRelay<UserData>>,
    queue: DynamicDeque<FeatureWorkerOutput<UserData, Control, Event, ToController>, 16>,
    shutdown: bool,
}

impl<UserData> Default for PubSubFeatureWorker<UserData> {
    fn default() -> Self {
        Self {
            relays: HashMap::new(),
            queue: Default::default(),
            shutdown: false,
        }
    }
}

impl<UserData: Eq + Copy + Debug> FeatureWorker<UserData, Control, Event, ToController, ToWorker<UserData>> for PubSubFeatureWorker<UserData> {
    fn on_network_raw(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, _conn: ConnId, remote: NetPair, _header: TransportMsgHeader, buf: Buffer) {
        log::debug!("[PubSubWorker] on_network_raw from {}", remote);
        let msg = return_if_err!(PubsubMessage::try_from(&buf as &[u8]));
        match msg {
            PubsubMessage::Control(relay_id, control) => {
                log::debug!("[PubSubWorker] received PubsubMessage::RelayControl({:?}, {:?})", relay_id, control);
                self.queue.push_back(FeatureWorkerOutput::ToController(ToController::RelayControl(remote, relay_id, control)));
            }
            PubsubMessage::SourceHint(channel, control) => {
                log::debug!("[PubSubWorker] received PubsubMessage::SourceHint({:?}, {:?})", channel, control);
                self.queue.push_back(FeatureWorkerOutput::ToController(ToController::SourceHint(remote, channel, control)));
            }
            PubsubMessage::Data(relay_id, data) => {
                log::debug!("[PubSubWorker] received PubsubMessage::Data({:?}, size {})", relay_id, data.len());
                let relay = return_if_none!(self.relays.get(&relay_id));
                // only relay from trusted source
                if relay.source == Some(remote) {
                    for actor in &relay.locals {
                        self.queue
                            .push_back(FeatureWorkerOutput::Event(*actor, Event(relay_id.0, ChannelEvent::SourceData(relay_id.1, data.to_vec()))));
                    }

                    if !relay.remotes.is_empty() {
                        let control = PubsubMessage::Data(relay_id, data);
                        //TODO avoid copy
                        self.queue.push_back(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), control.into()));
                    }
                } else {
                    log::warn!("[PubsubWorker] Relay from untrusted source local {:?} != remote {}", relay.source, remote);
                }
            }
        }
    }

    fn on_input(&mut self, ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<UserData, Control, ToWorker<UserData>>) {
        match input {
            FeatureWorkerInput::FromController(_, ToWorker::RelayControl(relay_id, control)) => match control {
                RelayWorkerControl::SendSub(uuid, remote) => {
                    let dest = if let Some(remote) = remote {
                        log::debug!("[PubsubWorker] SendSub for {:?} to {}", relay_id, remote);
                        remote
                    } else if let Some(next) = ctx.router.next(relay_id.1) {
                        log::debug!("[PubsubWorker] SendSub for {:?} to {} by select from router table", relay_id, next);
                        next
                    } else {
                        log::warn!("[PubsubWorker] SendSub: no route for {:?}", relay_id);
                        return;
                    };
                    let control = PubsubMessage::Control(relay_id, RelayControl::Sub(uuid));
                    self.queue.push_back(FeatureWorkerOutput::RawDirect2(dest, control.into()));
                }
                RelayWorkerControl::SendFeedback(fb, remote) => {
                    log::debug!("[PubsubWorker] SendFeedback for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::Feedback(fb));
                    self.queue.push_back(FeatureWorkerOutput::RawDirect2(remote, control.into()));
                }
                RelayWorkerControl::SendUnsub(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendUnsub for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::Unsub(uuid));
                    self.queue.push_back(FeatureWorkerOutput::RawDirect2(remote, control.into()));
                }
                RelayWorkerControl::SendSubOk(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendSubOk for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::SubOK(uuid));
                    self.queue.push_back(FeatureWorkerOutput::RawDirect2(remote, control.into()));
                }
                RelayWorkerControl::SendUnsubOk(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendUnsubOk for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::UnsubOK(uuid));
                    self.queue.push_back(FeatureWorkerOutput::RawDirect2(remote, control.into()));
                }
                RelayWorkerControl::SendRouteChanged => {
                    let relay = return_if_none!(self.relays.get(&relay_id));
                    log::debug!("[PubsubWorker] SendRouteChanged for {:?} to remotes {:?}", relay_id, relay.remotes);
                    for (addr, uuid) in relay.remotes_uuid.iter() {
                        let control = PubsubMessage::Control(relay_id, RelayControl::RouteChanged(*uuid));
                        self.queue.push_back(FeatureWorkerOutput::RawDirect2(*addr, control.into()));
                    }
                }
                RelayWorkerControl::RouteSetSource(source) => {
                    log::info!("[PubsubWorker] RouteSetSource for {:?} to {:?}", relay_id, source);
                    let entry: &mut WorkerRelay<UserData> = self.relays.entry(relay_id).or_insert(WorkerRelay {
                        source: None,
                        locals: vec![],
                        remotes: vec![],
                        remotes_uuid: HashMap::new(),
                    });

                    entry.source = Some(source);
                }
                RelayWorkerControl::RouteDelSource(source) => {
                    log::info!("[PubsubWorker] RouteDelSource for {:?} to {:?}", relay_id, source);
                    if let Some(entry) = self.relays.get_mut(&relay_id) {
                        if entry.source == Some(source) {
                            entry.source = None;
                        } else {
                            log::warn!("[PubsubWorker] RelayDel: relay {:?} source mismatch locked {:?} vs {}", relay_id, entry.source, source);
                        }
                    } else {
                        log::warn!("[PubsubWorker] RelayDel: relay not found {:?}", relay_id);
                    }
                }
                RelayWorkerControl::RouteSetLocal(actor) => {
                    log::debug!("[PubsubWorker] RouteSetLocal for {:?} to {:?}", relay_id, actor);
                    let entry: &mut WorkerRelay<UserData> = self.relays.entry(relay_id).or_insert(WorkerRelay {
                        source: None,
                        locals: vec![],
                        remotes: vec![],
                        remotes_uuid: HashMap::new(),
                    });

                    entry.locals.push(actor);
                }
                RelayWorkerControl::RouteDelLocal(actor) => {
                    log::debug!("[PubsubWorker] RouteDelLocal for {:?} to {:?}", relay_id, actor);
                    if let Some(entry) = self.relays.get_mut(&relay_id) {
                        if let Some(pos) = entry.locals.iter().position(|x| *x == actor) {
                            entry.locals.swap_remove(pos);
                        }
                        if entry.is_empty() {
                            self.relays.remove(&relay_id);
                        }
                    } else {
                        log::warn!("[PubsubWorker] RelayDelLocal: relay not found {:?}", relay_id);
                    }
                }
                RelayWorkerControl::RouteSetRemote(remote, uuid) => {
                    log::debug!("[PubsubWorker] RouteSetRemote for {:?} to {:?}", relay_id, remote);
                    let entry: &mut WorkerRelay<UserData> = self.relays.entry(relay_id).or_insert(WorkerRelay {
                        source: None,
                        locals: vec![],
                        remotes: vec![],
                        remotes_uuid: HashMap::new(),
                    });

                    entry.remotes.push(remote);
                    entry.remotes_uuid.insert(remote, uuid);
                }
                RelayWorkerControl::RouteDelRemote(remote) => {
                    log::debug!("[PubsubWorker] RouteDelRemote for {:?} to {:?}", relay_id, remote);
                    if let Some(entry) = self.relays.get_mut(&relay_id) {
                        if let Some(pos) = entry.remotes.iter().position(|x| *x == remote) {
                            entry.remotes.swap_remove(pos);
                        }
                        if entry.is_empty() {
                            self.relays.remove(&relay_id);
                        }
                    } else {
                        log::warn!("RelayDelSub: relay not found {:?}", relay_id);
                    }
                }
            },
            FeatureWorkerInput::FromController(_, ToWorker::SourceHint(channel, remote, data)) => {
                if let Some(remote) = remote {
                    let control = PubsubMessage::SourceHint(channel, data);
                    self.queue.push_back(FeatureWorkerOutput::RawDirect2(remote, control.into()));
                } else {
                    let next = ctx.router.path_to_key(*channel as u32);
                    if let RouteAction::Next(remote) = next {
                        let control = PubsubMessage::SourceHint(channel, data);
                        self.queue.push_back(FeatureWorkerOutput::RawDirect2(remote, control.into()));
                    }
                }
            }
            FeatureWorkerInput::FromController(_, ToWorker::RelayData(relay_id, data)) => {
                let relay = return_if_none!(self.relays.get(&relay_id));
                if relay.remotes.is_empty() {
                    log::warn!("RelayData: no remote for {:?}", relay_id);
                    return;
                }
                let control = PubsubMessage::Data(relay_id, data);
                self.queue.push_back(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), control.into()));
            }
            FeatureWorkerInput::Control(actor, control) => match control {
                Control(channel, ChannelControl::PubData(data)) => {
                    let relay_id = RelayId(channel, ctx.node_id);
                    let relay = return_if_none!(self.relays.get(&relay_id));

                    for actor in &relay.locals {
                        self.queue
                            .push_back(FeatureWorkerOutput::Event(*actor, Event(channel, ChannelEvent::SourceData(ctx.node_id, data.clone()))));
                    }

                    if !relay.remotes.is_empty() {
                        let control = PubsubMessage::Data(relay_id, data);
                        self.queue.push_back(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), control.into()));
                    }
                }
                _ => self.queue.push_back(FeatureWorkerOutput::ForwardControlToController(actor, control)),
            },
            _ => {}
        }
    }

    fn on_shutdown(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64) {
        self.shutdown = true;
    }
}

impl<UserData> TaskSwitcherChild<FeatureWorkerOutput<UserData, Control, Event, ToController>> for PubSubFeatureWorker<UserData> {
    type Time = u64;

    fn is_empty(&self) -> bool {
        self.shutdown && self.queue.is_empty()
    }

    fn empty_event(&self) -> FeatureWorkerOutput<UserData, Control, Event, ToController> {
        FeatureWorkerOutput::OnResourceEmpty
    }

    fn pop_output(&mut self, _now: u64) -> Option<FeatureWorkerOutput<UserData, Control, Event, ToController>> {
        self.queue.pop_front()
    }
}

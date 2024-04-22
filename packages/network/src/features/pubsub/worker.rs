use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    net::SocketAddr,
};

use atm0s_sdn_identity::ConnId;
use atm0s_sdn_router::{RouteAction, RouterTable};

use crate::base::{Buffer, FeatureControlActor, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, TransportMsgHeader};

use super::{
    msg::{PubsubMessage, RelayControl, RelayId},
    ChannelControl, ChannelEvent, Control, Event, RelayWorkerControl, ToController, ToWorker,
};

struct WorkerRelay<UserData> {
    source: Option<SocketAddr>,
    locals: Vec<FeatureControlActor<UserData>>,
    remotes: Vec<SocketAddr>,
    remotes_uuid: HashMap<SocketAddr, u64>,
}

impl<UserData> WorkerRelay<UserData> {
    pub fn is_empty(&self) -> bool {
        self.locals.is_empty() && self.remotes.is_empty()
    }
}

pub struct PubSubFeatureWorker<UserData> {
    relays: HashMap<RelayId, WorkerRelay<UserData>>,
    buf: [u8; 1500],
    queue: VecDeque<FeatureWorkerOutput<'static, UserData, Control, Event, ToController>>,
}

impl<UserData> PubSubFeatureWorker<UserData> {
    pub(crate) fn new() -> Self {
        Self {
            relays: HashMap::new(),
            buf: [0; 1500],
            queue: VecDeque::new(),
        }
    }
}

impl<UserData: Eq + Copy + Debug> FeatureWorker<UserData, Control, Event, ToController, ToWorker<UserData>> for PubSubFeatureWorker<UserData> {
    fn on_network_raw<'a>(
        &mut self,
        _ctx: &mut FeatureWorkerContext,
        _now: u64,
        _conn: ConnId,
        remote: SocketAddr,
        _header: TransportMsgHeader,
        buf: Buffer<'a>,
    ) -> Option<FeatureWorkerOutput<'a, UserData, Control, Event, ToController>> {
        log::debug!("[PubSubWorker] on_network_raw from {}", remote);
        let msg = PubsubMessage::try_from(&buf as &[u8]).ok()?;
        match msg {
            PubsubMessage::Control(relay_id, control) => {
                log::debug!("[PubSubWorker] received PubsubMessage::RelayControl({:?}, {:?})", relay_id, control);
                Some(FeatureWorkerOutput::ToController(ToController::RelayControl(remote, relay_id, control)))
            }
            PubsubMessage::SourceHint(channel, control) => {
                log::debug!("[PubSubWorker] received PubsubMessage::SourceHint({:?}, {:?})", channel, control);
                Some(FeatureWorkerOutput::ToController(ToController::SourceHint(remote, channel, control)))
            }
            PubsubMessage::Data(relay_id, data) => {
                log::debug!("[PubSubWorker] received PubsubMessage::Data({:?}, size {})", relay_id, data.len());
                let relay = self.relays.get(&relay_id)?;
                // only relay from trusted source
                if relay.source == Some(remote) {
                    for actor in &relay.locals {
                        self.queue
                            .push_back(FeatureWorkerOutput::Event(*actor, Event(relay_id.0, ChannelEvent::SourceData(relay_id.1, data.to_vec()))));
                    }

                    if !relay.remotes.is_empty() {
                        let control = PubsubMessage::Data(relay_id, data);
                        let size: usize = control.write_to(&mut self.buf)?;
                        //TODO avoid copy
                        Some(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), Buffer::from(self.buf[0..size].to_vec())))
                    } else {
                        //TODO avoid push temp to queue
                        self.queue.pop_front()
                    }
                } else {
                    log::warn!("[PubsubWorker] Relay from untrusted source local {:?} != remote {}", relay.source, remote);
                    None
                }
            }
        }
    }

    fn on_input<'a>(
        &mut self,
        ctx: &mut FeatureWorkerContext,
        _now: u64,
        input: FeatureWorkerInput<'a, UserData, Control, ToWorker<UserData>>,
    ) -> Option<FeatureWorkerOutput<'a, UserData, Control, Event, ToController>> {
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
                        return None;
                    };
                    let control = PubsubMessage::Control(relay_id, RelayControl::Sub(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(dest, Buffer::from(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendFeedback(fb, remote) => {
                    log::debug!("[PubsubWorker] SendFeedback for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::Feedback(fb));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, Buffer::from(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendUnsub(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendUnsub for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::Unsub(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, Buffer::from(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendSubOk(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendSubOk for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::SubOK(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, Buffer::from(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendUnsubOk(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendUnsubOk for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::UnsubOK(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, Buffer::from(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendRouteChanged => {
                    let relay = self.relays.get(&relay_id)?;
                    log::debug!("[PubsubWorker] SendRouteChanged for {:?} to remotes {:?}", relay_id, relay.remotes);
                    for (addr, uuid) in relay.remotes_uuid.iter() {
                        let control = PubsubMessage::Control(relay_id, RelayControl::RouteChanged(*uuid));
                        let size: usize = control.write_to(&mut self.buf)?;
                        //TODO avoid copy
                        self.queue.push_back(FeatureWorkerOutput::RawDirect2(*addr, Buffer::from(self.buf[0..size].to_vec())));
                    }
                    self.queue.pop_front()
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
                    None
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
                    None
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
                    None
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
                    None
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
                    None
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
                    None
                }
            },
            FeatureWorkerInput::FromController(_, ToWorker::SourceHint(channel, remote, data)) => {
                if let Some(remote) = remote {
                    let control = PubsubMessage::SourceHint(channel, data);
                    let size = control.write_to(&mut self.buf)?;
                    Some(FeatureWorkerOutput::RawDirect2(remote, Buffer::from(self.buf[0..size].to_vec())))
                } else {
                    let next = ctx.router.path_to_key(*channel as u32);
                    match next {
                        RouteAction::Next(remote) => {
                            let control = PubsubMessage::SourceHint(channel, data);
                            let size = control.write_to(&mut self.buf)?;
                            Some(FeatureWorkerOutput::RawDirect2(remote, Buffer::from(self.buf[0..size].to_vec())))
                        }
                        _ => None,
                    }
                }
            }
            FeatureWorkerInput::FromController(_, ToWorker::RelayData(relay_id, data)) => {
                let relay = self.relays.get(&relay_id)?;
                if relay.remotes.is_empty() {
                    log::warn!("RelayData: no remote for {:?}", relay_id);
                    return None;
                }
                let control = PubsubMessage::Data(relay_id, data);
                let size: usize = control.write_to(&mut self.buf)?;
                //TODO avoid copy
                Some(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), Buffer::from(self.buf[0..size].to_vec())))
            }
            FeatureWorkerInput::Control(actor, control) => match control {
                Control(channel, ChannelControl::PubData(data)) => {
                    let relay_id = RelayId(channel, ctx.node_id);
                    let relay = self.relays.get(&relay_id)?;

                    for actor in &relay.locals {
                        self.queue
                            .push_back(FeatureWorkerOutput::Event(*actor, Event(channel, ChannelEvent::SourceData(ctx.node_id, data.clone()))));
                    }

                    if !relay.remotes.is_empty() {
                        let control = PubsubMessage::Data(relay_id, data);
                        let size: usize = control.write_to(&mut self.buf)?;
                        //TODO avoid copy
                        Some(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), Buffer::from(self.buf[0..size].to_vec())))
                    } else {
                        //TODO avoid push temp to queue
                        self.queue.pop_front()
                    }
                }
                _ => Some(FeatureWorkerOutput::ForwardControlToController(actor, control)),
            },
            _ => None,
        }
    }

    fn pop_output<'a>(&mut self, _ctx: &mut FeatureWorkerContext) -> Option<FeatureWorkerOutput<'a, UserData, Control, Event, ToController>> {
        self.queue.pop_front()
    }
}

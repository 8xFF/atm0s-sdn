use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::RouterTable;

use crate::base::{FeatureControlActor, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer};

use super::{
    msg::{PubsubMessage, RelayControl, RelayId},
    ChannelControl, ChannelEvent, Control, Event, RelayWorkerControl, ToController, ToWorker, FEATURE_ID, FEATURE_NAME,
};

struct WorkerRelay {
    source: Option<SocketAddr>,
    locals: Vec<FeatureControlActor>,
    remotes: Vec<SocketAddr>,
    remotes_uuid: HashMap<SocketAddr, u64>,
}

pub struct PubSubFeatureWorker {
    node_id: NodeId,
    relays: HashMap<RelayId, WorkerRelay>,
    buf: [u8; 1500],
    queue: VecDeque<FeatureWorkerOutput<'static, Control, Event, ToController>>,
}

impl PubSubFeatureWorker {
    pub(crate) fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            relays: HashMap::new(),
            buf: [0; 1500],
            queue: VecDeque::new(),
        }
    }
}

impl FeatureWorker<Control, Event, ToController, ToWorker> for PubSubFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_network_raw<'a>(
        &mut self,
        _ctx: &mut FeatureWorkerContext,
        _now: u64,
        _conn: ConnId,
        remote: SocketAddr,
        _header_len: usize,
        buf: GenericBuffer<'a>,
    ) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        log::debug!("[PubSubWorker] on_network_raw from {}", remote);
        let msg = PubsubMessage::try_from(&buf as &[u8]).ok()?;
        match msg {
            PubsubMessage::Control(relay_id, control) => {
                log::debug!("[PubSubWorker] received PubsubMessage::Control({:?}, {:?})", relay_id, control);
                Some(FeatureWorkerOutput::ToController(ToController::RemoteControl(remote, relay_id, control)))
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
                        Some(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), GenericBuffer::Vec(self.buf[0..size].to_vec())))
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

    fn on_input<'a>(&mut self, ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<'a, Control, ToWorker>) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        match input {
            FeatureWorkerInput::FromController(ToWorker::RelayWorkerControl(relay_id, control)) => match control {
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
                    Some(FeatureWorkerOutput::RawDirect2(dest, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendUnsub(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendUnsub for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::Unsub(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendSubOk(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendSubOk for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::SubOK(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendUnsubOk(uuid, remote) => {
                    log::debug!("[PubsubWorker] SendUnsubOk for {:?} to {:?}", relay_id, remote);
                    let control = PubsubMessage::Control(relay_id, RelayControl::UnsubOK(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendRouteChanged => {
                    let relay = self.relays.get(&relay_id)?;
                    log::debug!("[PubsubWorker] SendRouteChanged for {:?} to remotes {:?}", relay_id, relay.remotes);
                    for (addr, uuid) in relay.remotes_uuid.iter() {
                        let control = PubsubMessage::Control(relay_id, RelayControl::RouteChanged(*uuid));
                        let size: usize = control.write_to(&mut self.buf)?;
                        //TODO avoid copy
                        self.queue.push_back(FeatureWorkerOutput::RawDirect2(*addr, GenericBuffer::Vec(self.buf[0..size].to_vec())));
                    }
                    self.queue.pop_front()
                }
                RelayWorkerControl::RouteSetSource(source) => {
                    log::info!("[PubsubWorker] RouteSetSource for {:?} to {:?}", relay_id, source);
                    let entry: &mut WorkerRelay = self.relays.entry(relay_id).or_insert(WorkerRelay {
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
                            //TODO remove if empty
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
                    let entry: &mut WorkerRelay = self.relays.entry(relay_id).or_insert(WorkerRelay {
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
                    } else {
                        log::warn!("[PubsubWorker] RelayDelLocal: relay not found {:?}", relay_id);
                    }
                    None
                }
                RelayWorkerControl::RouteSetRemote(remote, uuid) => {
                    log::debug!("[PubsubWorker] RouteSetRemote for {:?} to {:?}", relay_id, remote);
                    let entry: &mut WorkerRelay = self.relays.entry(relay_id).or_insert(WorkerRelay {
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
                    } else {
                        log::warn!("RelayDelSub: relay not found {:?}", relay_id);
                    }
                    None
                }
            },
            FeatureWorkerInput::FromController(ToWorker::RelayData(relay_id, data)) => {
                let relay = self.relays.get(&relay_id)?;
                if relay.remotes.is_empty() {
                    log::warn!("RelayData: no remote for {:?}", relay_id);
                    return None;
                }
                let control = PubsubMessage::Data(relay_id, data);
                let size: usize = control.write_to(&mut self.buf)?;
                //TODO avoid copy
                Some(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), GenericBuffer::Vec(self.buf[0..size].to_vec())))
            }
            FeatureWorkerInput::Control(actor, control) => match control {
                Control(channel, ChannelControl::PubData(data)) => {
                    let relay_id = RelayId(channel, self.node_id);
                    let relay = self.relays.get(&relay_id)?;

                    for actor in &relay.locals {
                        self.queue
                            .push_back(FeatureWorkerOutput::Event(*actor, Event(channel, ChannelEvent::SourceData(self.node_id, data.clone()))));
                    }

                    if !relay.remotes.is_empty() {
                        let control = PubsubMessage::Data(relay_id, data);
                        let size: usize = control.write_to(&mut self.buf)?;
                        //TODO avoid copy
                        Some(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), GenericBuffer::Vec(self.buf[0..size].to_vec())))
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

    fn pop_output<'a>(&mut self) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        self.queue.pop_front()
    }
}

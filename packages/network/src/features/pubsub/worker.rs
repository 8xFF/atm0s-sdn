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
    source: SocketAddr,
    locals: Vec<FeatureControlActor>,
    remotes: Vec<SocketAddr>,
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
        conn: ConnId,
        remote: SocketAddr,
        _header_len: usize,
        buf: GenericBuffer<'a>,
    ) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        let msg = PubsubMessage::try_from(&buf as &[u8]).ok()?;
        match msg {
            PubsubMessage::Control(relay_id, control) => Some(FeatureWorkerOutput::ToController(ToController::RemoteControl(remote, relay_id, control))),
            PubsubMessage::Data(relay_id, data) => {
                let relay = self.relays.get(&relay_id)?;
                // only relay from trusted source
                if relay.source == remote {
                    for actor in &relay.locals {
                        self.queue
                            .push_back(FeatureWorkerOutput::Event(*actor, Event(relay_id.0, ChannelEvent::SourceData(relay_id.1, data.to_vec()))));
                    }

                    if !relay.remotes.is_empty() {
                        let control = PubsubMessage::Data(relay_id, &data);
                        let size: usize = control.write_to(&mut self.buf)?;
                        //TODO avoid copy
                        Some(FeatureWorkerOutput::RawBroadcast2(relay.remotes.clone(), GenericBuffer::Vec(self.buf[0..size].to_vec())))
                    } else {
                        //TODO avoid push temp to queue
                        self.queue.pop_front()
                    }
                } else {
                    log::warn!("Relay from untrusted source {} != {}", relay.source, conn);
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
                        remote
                    } else if let Some(next) = ctx.router.next(relay_id.1) {
                        next
                    } else {
                        log::warn!("SendSub: no route for {:?}", relay_id);
                        return None;
                    };
                    let control = PubsubMessage::Control(relay_id, RelayControl::Sub(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(dest, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendUnsub(uuid, remote) => {
                    let control = PubsubMessage::Control(relay_id, RelayControl::Unsub(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendSubOk(uuid, remote) => {
                    let control = PubsubMessage::Control(relay_id, RelayControl::SubOK(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::SendUnsubOk(uuid, remote) => {
                    let control = PubsubMessage::Control(relay_id, RelayControl::UnsubOK(uuid));
                    let size: usize = control.write_to(&mut self.buf)?;
                    //TODO avoid copy
                    Some(FeatureWorkerOutput::RawDirect2(remote, GenericBuffer::Vec(self.buf[0..size].to_vec())))
                }
                RelayWorkerControl::RouteSet(source) => {
                    let entry = self.relays.entry(relay_id).or_insert(WorkerRelay {
                        source,
                        locals: vec![],
                        remotes: vec![],
                    });

                    entry.source = source;
                    None
                }
                RelayWorkerControl::RouteDel(source) => {
                    if let Some(entry) = self.relays.get(&relay_id) {
                        if entry.source == source {
                            self.relays.remove(&relay_id);
                        } else {
                            log::warn!("RelayDel: relay {:?} source mismatch locked {} vs {}", relay_id, entry.source, source);
                        }
                    } else {
                        log::warn!("RelayDel: relay not found {:?}", relay_id);
                    }
                    None
                }
                RelayWorkerControl::RouteSetLocal(actor) => {
                    if let Some(entry) = self.relays.get_mut(&relay_id) {
                        entry.locals.push(actor);
                    } else {
                        log::warn!("RelayAddLocal: relay not found {:?}", relay_id);
                    }
                    None
                }
                RelayWorkerControl::RouteDelLocal(actor) => {
                    if let Some(entry) = self.relays.get_mut(&relay_id) {
                        if let Some(pos) = entry.locals.iter().position(|x| *x == actor) {
                            entry.locals.swap_remove(pos);
                        }
                    } else {
                        log::warn!("RelayDelLocal: relay not found {:?}", relay_id);
                    }
                    None
                }
                RelayWorkerControl::RouteSetRemote(remote) => {
                    if let Some(entry) = self.relays.get_mut(&relay_id) {
                        entry.remotes.push(remote);
                    } else {
                        log::warn!("RelayAddSub: relay not found {:?}", relay_id);
                    }
                    None
                }
                RelayWorkerControl::RouteDelRemote(remote) => {
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
                let control = PubsubMessage::Data(relay_id, &data);
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
                        let control = PubsubMessage::Data(relay_id, &data);
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

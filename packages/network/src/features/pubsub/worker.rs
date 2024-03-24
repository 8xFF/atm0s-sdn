use std::{collections::HashMap, net::SocketAddr};

use atm0s_sdn_identity::ConnId;
use atm0s_sdn_router::RouterTable;

use crate::base::{FeatureControlActor, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer};

use super::{
    msg::{PubsubMessage, RelayId},
    Control, Event, RelayWorkerControl, ToController, ToWorker, FEATURE_ID, FEATURE_NAME,
};

struct WorkerRelay {
    source: SocketAddr,
    locals: Vec<FeatureControlActor>,
    remotes: Vec<SocketAddr>,
}

#[derive(Default)]
pub struct PubSubFeatureWorker {
    relays: HashMap<RelayId, WorkerRelay>,
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
            PubsubMessage::Data(relay_id, _payload) => {
                let relay = self.relays.get(&relay_id)?;
                // only relay from trusted source
                if relay.source == remote {
                    todo!()
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
                    todo!()
                }
                RelayWorkerControl::SendUnsub(uuid, remote) => {
                    todo!()
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
            FeatureWorkerInput::Control(actor, data) => {
                todo!()
            }
            _ => None,
        }
    }
}

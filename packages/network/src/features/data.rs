use std::{
    collections::{HashMap, VecDeque},
    marker::PhantomData,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use derivative::Derivative;
use serde::{Deserialize, Serialize};

use crate::base::{Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, NetIncomingMeta, NetOutgoingMeta};

pub const FEATURE_ID: u8 = 1;
pub const FEATURE_NAME: &str = "data_transfer";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Ping(NodeId),
    DataListen(u16),
    DataUnlisten(u16),
    DataSendRule(u16, RouteRule, NetOutgoingMeta, Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Pong(NodeId, Option<u16>),
    Recv(u16, NetIncomingMeta, Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Debug, Serialize, Deserialize)]
enum DataMsg {
    Ping { id: u64, ts: u64, from: NodeId },
    Pong { id: u64, ts: u64 },
    Data(u16, Vec<u8>),
}

pub struct DataFeature<UserData> {
    waits: HashMap<u64, (u64, FeatureControlActor<UserData>, NodeId)>,
    ping_seq: u64,
    queue: VecDeque<FeatureOutput<UserData, Event, ToWorker>>,
    data_dest: HashMap<u16, FeatureControlActor<UserData>>,
}

impl<UserData> Default for DataFeature<UserData> {
    fn default() -> Self {
        Self {
            waits: HashMap::new(),
            ping_seq: 0,
            queue: VecDeque::new(),
            data_dest: HashMap::new(),
        }
    }
}

impl<UserData: Copy> Feature<UserData, Control, Event, ToController, ToWorker> for DataFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Tick(_) => {
                //clean timeout ping
                let mut timeout_list = Vec::new();
                for (id, (sent_ms, _, _)) in self.waits.iter() {
                    if now >= sent_ms + 2000 {
                        timeout_list.push(*id);
                    }
                }

                for id in timeout_list {
                    let (_, actor, dest) = self.waits.remove(&id).expect("Should have");
                    self.queue.push_back(FeatureOutput::Event(actor, Event::Pong(dest, None)));
                }
            }
            _ => {}
        }
    }

    fn on_input<'a>(&mut self, ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'a, UserData, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => match control {
                Control::Ping(dest) => {
                    log::info!("[DataFeature] Sending Ping to: {}", dest);
                    let seq = self.ping_seq;
                    self.ping_seq += 1;
                    self.waits.insert(seq, (now_ms, actor, dest));
                    let msg = bincode::serialize(&DataMsg::Ping {
                        id: seq,
                        ts: now_ms,
                        from: ctx.node_id,
                    })
                    .expect("should work");
                    let rule = RouteRule::ToNode(dest);
                    self.queue.push_back(FeatureOutput::SendRoute(rule, NetOutgoingMeta::default(), msg.into()));
                }
                Control::DataListen(port) => {
                    self.data_dest.insert(port, actor);
                }
                Control::DataUnlisten(port) => {
                    self.data_dest.remove(&port);
                }
                Control::DataSendRule(port, rule, ttl, data) => {
                    let data = DataMsg::Data(port, data);
                    let msg = bincode::serialize(&data).expect("should work");
                    self.queue.push_back(FeatureOutput::SendRoute(rule, ttl, msg.into()));
                }
            },
            FeatureInput::Net(_, meta, buf) | FeatureInput::Local(meta, buf) => {
                if let Ok(msg) = bincode::deserialize::<DataMsg>(&buf) {
                    match msg {
                        DataMsg::Pong { id, ts } => {
                            if let Some((_, actor, dest)) = self.waits.remove(&id) {
                                self.queue.push_back(FeatureOutput::Event(actor, Event::Pong(dest, Some((now_ms - ts) as u16))));
                            } else {
                                log::warn!("[DataFeature] Pong with unknown id: {}", id);
                            }
                        }
                        DataMsg::Ping { id, ts, from } => {
                            log::info!("[DataFeature] Got ping from: {}", from);
                            let msg = bincode::serialize(&DataMsg::Pong { id, ts }).expect("should work");
                            let rule = RouteRule::ToNode(from);
                            self.queue.push_back(FeatureOutput::SendRoute(rule, NetOutgoingMeta::default(), msg.into()));
                        }
                        DataMsg::Data(port, data) => {
                            if let Some(actor) = self.data_dest.get(&port) {
                                self.queue.push_back(FeatureOutput::Event(*actor, Event::Recv(port, meta, data)));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn pop_output<'a>(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<UserData, Event, ToWorker>> {
        self.queue.pop_front()
    }
}

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct DataFeatureWorker<UserData> {
    _tmp: PhantomData<UserData>,
}

impl<UserData> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for DataFeatureWorker<UserData> {}

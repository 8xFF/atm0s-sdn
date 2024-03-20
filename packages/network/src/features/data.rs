use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use serde::{Deserialize, Serialize};

use crate::base::{Feature, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, Ttl};

pub const FEATURE_ID: u8 = 1;
pub const FEATURE_NAME: &str = "data_transfer";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Ping(NodeId),
    SendRule(RouteRule, Ttl, Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Pong(NodeId, Option<u16>),
    Recv(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

#[derive(Debug, Serialize, Deserialize)]
enum DataMsg {
    Ping { id: u64, ts: u64, from: NodeId },
    Pong { id: u64, ts: u64 },
    DataController { data: Vec<u8> },
    DataService { service: u8, data: Vec<u8> },
}

pub struct DataFeature {
    node_id: NodeId,
    waits: HashMap<u64, (u64, FeatureControlActor, NodeId)>,
    ping_seq: u64,
    queue: VecDeque<FeatureOutput<Event, ToWorker>>,
}

impl DataFeature {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            waits: HashMap::new(),
            ping_seq: 0,
            queue: VecDeque::new(),
        }
    }
}

impl Feature<Control, Event, ToController, ToWorker> for DataFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, now: u64, input: FeatureSharedInput) {
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

    fn on_input<'a>(&mut self, now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
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
                        from: self.node_id,
                    })
                    .expect("should work");
                    let rule = RouteRule::ToNode(dest);
                    self.queue.push_back(FeatureOutput::SendRoute(rule, Ttl::default(), msg.into()));
                }
                Control::SendRule(rule, ttl, data) => {
                    let data = match actor {
                        FeatureControlActor::Controller => DataMsg::DataController { data },
                        FeatureControlActor::Service(service) => DataMsg::DataService { service: *service, data },
                    };
                    let msg = bincode::serialize(&data).expect("should work");
                    self.queue.push_back(FeatureOutput::SendRoute(rule, ttl, msg.into()));
                }
            },
            FeatureInput::Net(_, buf) | FeatureInput::Local(buf) => {
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
                            self.queue.push_back(FeatureOutput::SendRoute(rule, Ttl::default(), msg.into()));
                        }
                        DataMsg::DataController { data } => {
                            self.queue.push_back(FeatureOutput::Event(FeatureControlActor::Controller, Event::Recv(data)));
                        }
                        DataMsg::DataService { service, data } => {
                            self.queue.push_back(FeatureOutput::Event(FeatureControlActor::Service(service.into()), Event::Recv(data)));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn pop_output<'a>(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        self.queue.pop_front()
    }
}

#[derive(Default)]
pub struct DataFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for DataFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }
}

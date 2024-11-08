use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use derivative::Derivative;
use sans_io_runtime::{collections::DynamicDeque, TaskSwitcherChild};
use serde::{Deserialize, Serialize};

use crate::base::{
    Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerInput, FeatureWorkerOutput, NetIncomingMeta, NetOutgoingMeta,
};

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

pub type Output<UserData> = FeatureOutput<UserData, Event, ToWorker>;
pub type WorkerOutput<UserData> = FeatureWorkerOutput<UserData, Control, Event, ToController>;

pub struct DataFeature<UserData> {
    waits: HashMap<u64, (u64, FeatureControlActor<UserData>, NodeId)>,
    ping_seq: u64,
    queue: VecDeque<Output<UserData>>,
    data_dest: HashMap<u16, FeatureControlActor<UserData>>,
    shutdown: bool,
}

impl<UserData> Default for DataFeature<UserData> {
    fn default() -> Self {
        Self {
            waits: HashMap::new(),
            ping_seq: 0,
            queue: VecDeque::new(),
            data_dest: HashMap::new(),
            shutdown: false,
        }
    }
}

impl<UserData: Copy> Feature<UserData, Control, Event, ToController, ToWorker> for DataFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, now: u64, input: FeatureSharedInput) {
        if let FeatureSharedInput::Tick(_) = input {
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
    }

    fn on_input(&mut self, ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'_, UserData, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => match control {
                Control::Ping(dest) => {
                    log::info!("[DataFeature] send ping to: {}", dest);
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
                log::debug!("[DataFeature] on message from {:?} len {}", meta.source, buf.len());
                if let Ok(msg) = bincode::deserialize::<DataMsg>(&buf) {
                    match msg {
                        DataMsg::Pong { id, ts } => {
                            if let Some((_, actor, dest)) = self.waits.remove(&id) {
                                self.queue.push_back(FeatureOutput::Event(actor, Event::Pong(dest, Some((now_ms - ts) as u16))));
                            } else {
                                log::warn!("[DataFeature] pong with unknown id: {}", id);
                            }
                        }
                        DataMsg::Ping { id, ts, from } => {
                            log::info!("[DataFeature] got ping from: {}", from);
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

    fn on_shutdown(&mut self, _ctx: &FeatureContext, _now: u64) {
        self.shutdown = true;
    }
}

impl<UserData> TaskSwitcherChild<Output<UserData>> for DataFeature<UserData> {
    type Time = u64;

    fn is_empty(&self) -> bool {
        self.shutdown && self.queue.is_empty()
    }

    fn empty_event(&self) -> Output<UserData> {
        Output::OnResourceEmpty
    }

    fn pop_output(&mut self, _now: u64) -> Option<Output<UserData>> {
        self.queue.pop_front()
    }
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
pub struct DataFeatureWorker<UserData> {
    queue: DynamicDeque<WorkerOutput<UserData>, 1>,
    shutdown: bool,
}

impl<UserData: Debug> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for DataFeatureWorker<UserData> {
    fn on_input(&mut self, _ctx: &mut crate::base::FeatureWorkerContext, _now: u64, input: crate::base::FeatureWorkerInput<UserData, Control, ToWorker>) {
        match input {
            FeatureWorkerInput::Control(actor, control) => self.queue.push_back(FeatureWorkerOutput::ForwardControlToController(actor, control)),
            FeatureWorkerInput::Network(conn, header, buf) => self.queue.push_back(FeatureWorkerOutput::ForwardNetworkToController(conn, header, buf)),
            #[cfg(feature = "vpn")]
            FeatureWorkerInput::TunPkt(..) => {}
            FeatureWorkerInput::FromController(..) => {
                log::warn!("No handler for FromController");
            }
            FeatureWorkerInput::Local(header, buf) => self.queue.push_back(FeatureWorkerOutput::ForwardLocalToController(header, buf)),
        }
    }

    fn on_shutdown(&mut self, _ctx: &mut crate::base::FeatureWorkerContext, _now: u64) {
        log::info!("[DataFeatureWorker] Shutdown");
        self.shutdown = true;
    }
}

impl<UserData> TaskSwitcherChild<WorkerOutput<UserData>> for DataFeatureWorker<UserData> {
    type Time = u64;

    fn is_empty(&self) -> bool {
        self.shutdown && self.queue.is_empty()
    }

    fn empty_event(&self) -> WorkerOutput<UserData> {
        WorkerOutput::OnResourceEmpty
    }

    fn pop_output(&mut self, _now: u64) -> Option<WorkerOutput<UserData>> {
        self.queue.pop_front()
    }
}

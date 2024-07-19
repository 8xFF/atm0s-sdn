//! DHT-based key-value storage feature.
//!
//! This is dead-simple DHT Key-Value, which implements multi-sub key inside a key.
//! A value is a map which is stored in a node with route key is key as u32.
//!
//! For solve conflict, each sub_key will attacked to a locked value, which is a pair (node, lock_session).
//! In which, node is the node that locked the value, and session is the session of the lock.

use std::fmt::Debug;

use atm0s_sdn_identity::NodeId;
use derivative::Derivative;
use sans_io_runtime::{collections::DynamicDeque, TaskSwitcherChild};

use crate::base::{Feature, FeatureContext, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerInput, FeatureWorkerOutput, NetOutgoingMeta};

use self::{
    internal::InternalOutput,
    msg::{NodeSession, Version},
};

mod client;
mod internal;
mod msg;
mod server;

pub use self::msg::{Key, Map};

pub const FEATURE_ID: u8 = 4;
pub const FEATURE_NAME: &str = "dht_kv";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapControl {
    Set(Key, Vec<u8>),
    Del(Key),
    Sub,
    Unsub,
}

impl MapControl {
    pub fn is_creator(&self) -> bool {
        matches!(self, MapControl::Set(_, _) | MapControl::Sub)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    MapCmd(Map, MapControl),
    MapGet(Map),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetError {
    Timeout,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapEvent {
    OnSet(Key, NodeId, Vec<u8>),
    OnDel(Key, NodeId),
    OnRelaySelected(NodeId),
}

type EventMapGetRs = Result<Vec<(Key, NodeSession, Version, Vec<u8>)>, GetError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    MapEvent(Map, MapEvent),
    MapGetRes(Map, EventMapGetRs),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

pub type Output<UserData> = FeatureOutput<UserData, Event, ToWorker>;
pub type WorkerOutput<UserData> = FeatureWorkerOutput<UserData, Control, Event, ToController>;

pub struct DhtKvFeature<UserData> {
    internal: internal::DhtKvInternal<UserData>,
}

impl<UserData: Eq + Copy + Debug> DhtKvFeature<UserData> {
    pub fn new(node_id: NodeId, session: u64) -> Self {
        Self {
            internal: internal::DhtKvInternal::new(NodeSession(node_id, session)),
        }
    }
}

impl<UserData: Eq + Copy + Debug> Feature<UserData, Control, Event, ToController, ToWorker> for DhtKvFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, now: u64, input: FeatureSharedInput) {
        if let FeatureSharedInput::Tick(_) = input {
            self.internal.on_tick(now);
        }
    }

    fn on_input<'a>(&mut self, _ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'a, UserData, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => {
                log::debug!("[DhtKv] on ext input: actor={:?}, control={:?}", actor, control);
                self.internal.on_local(now_ms, actor, control);
            }
            FeatureInput::Local(_header, buf) => {
                if let Ok(cmd) = bincode::deserialize(&buf) {
                    self.internal.on_remote(now_ms, cmd)
                }
            }
            FeatureInput::Net(_conn, meta, buf) => {
                if !meta.secure {
                    //only allow secure message
                    log::warn!("[DhtKv] reject unsecure message");
                    return;
                }
                if let Ok(cmd) = bincode::deserialize(&buf) {
                    self.internal.on_remote(now_ms, cmd)
                }
            }
            _ => {}
        }
    }
}

impl<UserData: Eq + Copy + Debug> TaskSwitcherChild<FeatureOutput<UserData, Event, ToWorker>> for DhtKvFeature<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<FeatureOutput<UserData, Event, ToWorker>> {
        match self.internal.pop_action()? {
            InternalOutput::Local(service, event) => Some(FeatureOutput::Event(service, event)),
            InternalOutput::Remote(rule, cmd) => Some(FeatureOutput::SendRoute(
                rule,
                NetOutgoingMeta::new(false, Default::default(), 0, true),
                bincode::serialize(&cmd).expect("Should to bytes").into(),
            )),
        }
    }
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
pub struct DhtKvFeatureWorker<UserData> {
    queue: DynamicDeque<FeatureWorkerOutput<UserData, Control, Event, ToController>, 1>,
}

impl<UserData> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for DhtKvFeatureWorker<UserData> {
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
}

impl<UserData> TaskSwitcherChild<FeatureWorkerOutput<UserData, Control, Event, ToController>> for DhtKvFeatureWorker<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<FeatureWorkerOutput<UserData, Control, Event, ToController>> {
        self.queue.pop_front()
    }
}

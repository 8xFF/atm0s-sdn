//! DHT-based key-value storage feature.
//!
//! This is dead-simple DHT Key-Value, which implements multi-sub key inside a key.
//! A value is a map which is stored in a node with route key is key as u32.
//!
//! For solve conflict, each sub_key will attacked to a locked value, which is a pair (node, lock_session).
//! In which, node is the node that locked the value, and session is the session of the lock.

use atm0s_sdn_identity::NodeId;

use crate::base::{Feature, FeatureContext, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, NetOutgoingMeta, Ttl};

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
        match self {
            MapControl::Set(_, _) => true,
            MapControl::Sub => true,
            _ => false,
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    MapEvent(Map, MapEvent),
    MapGetRes(Map, Result<Vec<(Key, NodeSession, Version, Vec<u8>)>, GetError>),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

pub struct DhtKvFeature {
    internal: internal::DhtKvInternal,
}

impl DhtKvFeature {
    pub fn new(node_id: NodeId, session: u64) -> Self {
        Self {
            internal: internal::DhtKvInternal::new(NodeSession(node_id, session)),
        }
    }
}

impl Feature<Control, Event, ToController, ToWorker> for DhtKvFeature {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Tick(_) => {
                self.internal.on_tick(now);
            }
            _ => {}
        }
    }

    fn on_input<'a>(&mut self, _ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
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
            FeatureInput::Net(_conn, _header, buf) => {
                if let Ok(cmd) = bincode::deserialize(&buf) {
                    self.internal.on_remote(now_ms, cmd)
                }
            }
            _ => {}
        }
    }

    fn pop_output<'a>(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<Event, ToWorker>> {
        match self.internal.pop_action()? {
            InternalOutput::Local(service, event) => Some(FeatureOutput::Event(service, event)),
            InternalOutput::Remote(rule, cmd) => Some(FeatureOutput::SendRoute(rule, NetOutgoingMeta::default(), bincode::serialize(&cmd).expect("Should to bytes"))),
        }
    }
}

#[derive(Default)]
pub struct DhtKvFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for DhtKvFeatureWorker {}

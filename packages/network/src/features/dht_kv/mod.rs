//! DHT-based key-value storage feature.
//!
//! This is dead-simple DHT Key-Value, which implements multi-sub key inside a key.
//! A value is a map which is stored in a node with route key is key as u32.
//!
//! For solve conflict, each sub_key will attacked to a locked value, which is a pair (node, lock_session).
//! In which, node is the node that locked the value, and session is the session of the lock.

use atm0s_sdn_identity::NodeId;

use crate::base::{Feature, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker};

use self::{
    internal::InternalOutput,
    msg::{Key, NodeSession, SubKey},
};

mod client;
mod internal;
mod msg;
mod seq;
mod server;

pub const FEATURE_ID: u8 = 4;
pub const FEATURE_NAME: &str = "dht_kv";

#[derive(Debug, Clone)]
pub enum Control {
    HSet(Key, SubKey, Vec<u8>),
    HGet(Key, SubKey),
    HDel(Key, SubKey),
    HSub(Key),
    HUnsub(Key),
}

#[derive(Debug, Clone)]
pub enum HGetError {
    Timeout,
    NotFound,
}

#[derive(Debug, Clone)]
pub enum Event {
    HSetOk(Key, SubKey, NodeId),
    HDelOk(Key, SubKey, NodeId),
    HGetOk(Key, Result<(Vec<(SubKey, Vec<u8>, NodeId, u64)>, NodeId), HGetError>),
    OnHSet(Key, SubKey, NodeId, Vec<u8>),
    OnHDel(Key, SubKey, NodeId),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

pub struct DhtKvFeature {
    node_id: NodeId,
    internal: internal::DhtKvInternal,
}

impl DhtKvFeature {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            internal: internal::DhtKvInternal::new(NodeSession(node_id, 0)),
        }
    }
}

impl Feature<Control, Event, ToController, ToWorker> for DhtKvFeature {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }

    fn on_shared_input(&mut self, now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Tick(_) => {
                self.internal.on_tick(now);
            }
            _ => {}
        }
    }

    fn on_input<'a>(&mut self, now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Control(service, control) => {
                self.internal.on_local(now_ms, service, control);
            }
            FeatureInput::ForwardLocalFromWorker(buf) => {
                if let Ok(cmd) = bincode::deserialize(&buf) {
                    self.internal.on_remote(now_ms, cmd)
                }
            }
            FeatureInput::ForwardNetFromWorker(_conn, buf) => {
                if let Ok(cmd) = bincode::deserialize(&buf) {
                    self.internal.on_remote(now_ms, cmd)
                }
            }
            _ => {}
        }
    }

    fn pop_output<'a>(&mut self) -> Option<FeatureOutput<Event, ToWorker>> {
        match self.internal.pop_action()? {
            InternalOutput::Local(service, event) => Some(FeatureOutput::Event(service, event)),
            InternalOutput::Remote(rule, cmd) => Some(FeatureOutput::SendRoute(rule, bincode::serialize(&cmd).expect("Should to bytes"))),
        }
    }
}

#[derive(Default)]
pub struct DhtKvFeatureWorker {}

impl FeatureWorker<Control, Event, ToController, ToWorker> for DhtKvFeatureWorker {
    fn feature_type(&self) -> u8 {
        FEATURE_ID
    }

    fn feature_name(&self) -> &str {
        FEATURE_NAME
    }
}

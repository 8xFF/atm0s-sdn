use std::sync::Arc;

use bluesea_identity::NodeId;
use network::convert_enum;
use utils::random::Random;

use crate::{
    logic::key_value::{KeyValueClientEvent, KeyValueServerAction},
    KeyId,
};

pub enum KeyValueBehaviorEvent {
    FromNode(KeyValueMsg),
}

pub enum KeyValueHandlerEvent {
    
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyValueMsg {
    KeyValueServer(u64, NodeId, NodeId, KeyValueServerAction),
    KeyValueClient(u64, NodeId, NodeId, KeyValueClientEvent),
    Ack(u64, NodeId),
}

pub enum KeyValueReq {}

pub enum KeyValueRes {}

#[derive(convert_enum::From, convert_enum::TryInto, Debug, PartialEq, Eq)]
pub enum SubAction {
    KeyValueServer(KeyValueServerAction),
    KeyValueClient(KeyValueClientEvent),
}

#[derive(Debug, PartialEq, Eq)]
pub enum StorageActionRouting {
    Node(NodeId),
    ClosestNode(KeyId),
}

impl StorageActionRouting {
    pub fn routing_key(&self) -> NodeId {
        match self {
            Self::Node(node_id) => *node_id,
            Self::ClosestNode(key_id) => *key_id as u32,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StorageActionRetryStrategy {
    Retry(u32),
    NoRetry,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StorageAction {
    pub(crate) routing: StorageActionRouting,
    pub(crate) action_id: u64,
    pub(crate) target_id: KeyId,
    pub(crate) retry: StorageActionRetryStrategy,
    pub(crate) sub_action: SubAction,
}

impl StorageAction {
    pub fn make<A>(
        random: &Arc<dyn Random<u64>>,
        routing: StorageActionRouting,
        target_id: KeyId,
        action: A,
        retry: StorageActionRetryStrategy,
    ) -> Self
    where
        A: Into<SubAction>,
    {
        Self {
            routing,
            action_id: random.random(),
            target_id,
            retry,
            sub_action: action.into(),
        }
    }
}

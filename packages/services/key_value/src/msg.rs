use std::sync::Arc;

use bluesea_identity::NodeId;
use network::convert_enum;
use utils::random::Random;

use crate::{
    logic::key_value::{KeyValueClientEvents, KeyValueServerActions},
    KeyId,
};

pub enum KeyValueBehaviorEvent {}

pub enum KeyValueHandlerEvent {}

pub enum KeyValueMsg {}

pub enum KeyValueReq {}

pub enum KeyValueRes {}

#[derive(convert_enum::From, convert_enum::TryInto, Debug, PartialEq, Eq)]
pub enum SubAction {
    KeyValueServer(KeyValueServerActions),
    KeyValueClient(KeyValueClientEvents),
}

#[derive(Debug, PartialEq, Eq)]
pub enum StorageActionRouting {
    Node(NodeId),
    ClosestNode(KeyId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum StorageActionRetryStrategy {
    Retry(u32),
    NoRetry,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StorageAction {
    routing: StorageActionRouting,
    action_id: u64,
    target_id: KeyId,
    retry: StorageActionRetryStrategy,
    sub_action: SubAction,
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

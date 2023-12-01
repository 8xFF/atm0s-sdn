use serde::{Deserialize, Serialize};

use crate::RpcError;

/// This enum is used to serialize and deseriaze data tranfer over network
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RpcTransmitEvent {
    Event(u8, Vec<u8>),
    Request(u8, u64, Vec<u8>),
    Response(u8, u64, Result<Vec<u8>, RpcError>),
}

use bluesea_identity::NodeId;

use crate::{KeyId, ValueType};

pub mod client;
pub mod server;

/// This storage is used to store key-value pairs in a database.
/// Logic: external runtime call actions or tick then pull actions from the queue and execute them.
/// This storage implement bellow types:
///     - Simple Key Value with version
///     - Multi Key Value with version
///
/// Two types of data
///     - Local Data ( this is used to store data in the current runtime )
///     - Remote Data ( this is used to store data from other nodes )
///
/// Two types of output actions
///     - SyncSet ( this is used to sync data to other nodes )
///     - SyncDel ( this is used to sync data to other nodes )
///     - NotifySet ( this is used to notify other nodes about data changed )
///     - NotifyDel ( this is used to notify other nodes about data changed )
///     - NotifyChangedOwner ( this is used to notify other nodes about data changed owner )
///
/// This storage implement bellow functions:
///   - set_local(key, value, version, ex): this function set value in local mode and create 3 SyncSet actions with 3 sync keys
///   - get(key): this function get value from local mode or remote mode
///   - del_local(key): this function delete value from local mode and create 3 SyncDel actions with 3 sync keys
///   - on_remote(actions)
///   - poll() -> Action
///
/// Action and Ack: Each action has uuid and when action is executed, it will create an ack with the same uuid and response to
/// source of action. If ack is not received in a period of time, the action will be executed again.

#[derive(Debug, PartialEq, Eq)]
pub enum KeyValueServerActions {
    Set(KeyId, ValueType, u64, Option<u64>),
    Del(KeyId, u64),
    Sub(KeyId, NodeId, Option<u64>),
    UnSub(KeyId, NodeId),
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyValueClientEvents {
    NotifySet(KeyId, ValueType, u64),
    NotifyDel(KeyId, u64),
}

// pub enum GetKeyError {
//     NotFound,
// }

// pub enum GetResult<'a> {
//     Local(&'a [u8], u64),
//     RemoteHas(&'a [u8], u64),
//     Remote(Receiver<Result<(Vec<u8>, u64), GetKeyError>>),
// }

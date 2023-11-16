use crate::{KeyId, KeySource, KeyVersion, ReqId, SubKeyId, ValueType};
use bluesea_identity::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq)]
pub enum KeyValueBehaviorEvent {
    FromNode(NodeId, KeyValueMsg),
    Awake,
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyValueHandlerEvent {}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum KeyValueSdkEventError {
    NetworkError,
    Timeout,
    InternalError,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum KeyValueSdkEvent {
    Get(u64, KeyId, u64),
    GetH(u64, KeyId, u64),
    Set(KeyId, ValueType, Option<u64>),
    SetH(KeyId, SubKeyId, ValueType, Option<u64>),
    Del(KeyId),
    DelH(KeyId, SubKeyId),
    Sub(u64, KeyId, Option<u64>),
    SubH(u64, KeyId, Option<u64>),
    Unsub(u64, KeyId),
    UnsubH(u64, KeyId),
    OnGet(u64, KeyId, Result<Option<(ValueType, KeyVersion, KeySource)>, KeyValueSdkEventError>),
    OnGetH(u64, KeyId, Result<Option<Vec<(SubKeyId, ValueType, KeyVersion, KeySource)>>, KeyValueSdkEventError>),
    OnKeyChanged(u64, KeyId, Option<ValueType>, KeyVersion, KeySource),
    OnKeyHChanged(u64, KeyId, SubKeyId, Option<ValueType>, KeyVersion, KeySource),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimpleRemoteEvent {
    /// Set sub key of key
    Set(ReqId, KeyId, ValueType, KeyVersion, Option<u64>),
    /// Get key with specific sub key or all sub keys if not specified
    Get(ReqId, KeyId),
    /// Delete key with specific sub key or all sub keys created by requrested node if not specified
    /// If KeyVersion is greater or equal current stored version then that key will be deleted. Otherwise, nothing will happen and return Ack with NoneKeyVersion
    Del(ReqId, KeyId, KeyVersion),
    Sub(ReqId, KeyId, Option<u64>),
    Unsub(ReqId, KeyId),
    OnKeySetAck(ReqId),
    OnKeyDelAck(ReqId),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub enum SimpleLocalEvent {
    /// Response set request with key and version, if success => true, otherwise => false
    SetAck(ReqId, KeyId, KeyVersion, bool),
    GetAck(ReqId, KeyId, Option<(ValueType, KeyVersion, KeySource)>),
    DelAck(ReqId, KeyId, Option<KeyVersion>),
    SubAck(ReqId, KeyId),
    /// Response unsub request with key, if success => true, otherwise => false
    UnsubAck(ReqId, KeyId, bool),
    OnKeySet(ReqId, KeyId, ValueType, KeyVersion, KeySource),
    OnKeyDel(ReqId, KeyId, KeyVersion, KeySource),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HashmapRemoteEvent {
    /// Set sub key of key
    Set(ReqId, KeyId, SubKeyId, ValueType, KeyVersion, Option<u64>),
    /// Get key with specific sub key or all sub keys if not specified
    Get(ReqId, KeyId),
    /// Delete key with specific sub key or all sub keys created by requrested node if not specified
    /// If KeyVersion is greater or equal current stored version then that key will be deleted. Otherwise, nothing will happen and return Ack with NoneKeyVersion
    Del(ReqId, KeyId, SubKeyId, KeyVersion),
    Sub(ReqId, KeyId, Option<u64>),
    Unsub(ReqId, KeyId),
    OnKeySetAck(ReqId),
    OnKeyDelAck(ReqId),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone)]
pub enum HashmapLocalEvent {
    /// Response set request with key and version, if success => true, otherwise => false
    SetAck(ReqId, KeyId, SubKeyId, KeyVersion, bool),
    GetAck(ReqId, KeyId, Option<Vec<(SubKeyId, ValueType, KeyVersion, KeySource)>>),
    DelAck(ReqId, KeyId, SubKeyId, Option<KeyVersion>),
    SubAck(ReqId, KeyId),
    /// Response unsub request with key, if success => true, otherwise => false
    UnsubAck(ReqId, KeyId, bool),
    OnKeySet(ReqId, KeyId, SubKeyId, ValueType, KeyVersion, KeySource),
    OnKeyDel(ReqId, KeyId, SubKeyId, KeyVersion, KeySource),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyValueMsg {
    SimpleRemote(SimpleRemoteEvent),
    SimpleLocal(SimpleLocalEvent),
    HashmapRemote(HashmapRemoteEvent),
    HashmapLocal(HashmapLocalEvent),
}

use crate::{KeyId, KeyVersion, ReqId, ValueType};
use network::msg::MsgHeader;
use serde::{Deserialize, Serialize};
pub enum KeyValueBehaviorEvent {
    FromNode(MsgHeader, KeyValueMsg),
}

pub enum KeyValueHandlerEvent {}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteEvent {
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
pub enum LocalEvent {
    /// Response set request with key and version, if success => true, otherwise => false
    SetAck(ReqId, KeyId, KeyVersion, bool),
    GetAck(ReqId, KeyId, Option<(ValueType, KeyVersion)>),
    DelAck(ReqId, KeyId, Option<KeyVersion>),
    SubAck(ReqId, KeyId),
    /// Response unsub request with key, if success => true, otherwise => false
    UnsubAck(ReqId, KeyId, bool),
    OnKeySet(ReqId, KeyId, ValueType, KeyVersion),
    OnKeyDel(ReqId, KeyId, KeyVersion),
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyValueMsg {
    Remote(RemoteEvent),
    Local(LocalEvent),
}

pub enum KeyValueReq {}

pub enum KeyValueRes {}

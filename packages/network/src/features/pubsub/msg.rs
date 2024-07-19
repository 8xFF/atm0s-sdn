use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::simple_pub_type;
use sans_io_runtime::Buffer;
use serde::{Deserialize, Serialize};

use crate::base::{TransportMsg, TransportMsgHeader, TransportMsgHeaderError};

use super::FEATURE_ID;

simple_pub_type!(ChannelId, u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayId(pub ChannelId, pub NodeId);

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Feedback {
    pub kind: u8,
    pub count: u64,
    pub max: u64,
    pub min: u64,
    pub sum: u64,
    pub interval_ms: u16,
    pub timeout_ms: u16,
}

impl Feedback {
    pub fn simple(kind: u8, value: u64, interval_ms: u16, timeout_ms: u16) -> Self {
        Feedback {
            kind,
            count: 1,
            max: value,
            min: value,
            sum: value,
            interval_ms,
            timeout_ms,
        }
    }
}

///implement add to Feedback
impl std::ops::Add for Feedback {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Feedback {
            kind: self.kind,
            count: self.count + rhs.count,
            max: self.max.max(rhs.max),
            min: self.min.min(rhs.min),
            sum: self.sum + rhs.sum,
            interval_ms: self.interval_ms.min(rhs.interval_ms),
            timeout_ms: self.timeout_ms.max(rhs.timeout_ms),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelayControl {
    Sub(u64),
    Unsub(u64),
    SubOK(u64),
    UnsubOK(u64),
    RouteChanged(u64),
    Feedback(Feedback),
}

impl RelayControl {
    pub fn should_create(&self) -> bool {
        matches!(self, RelayControl::Sub(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceHint {
    /// This is used to notify a source is new or still alive.
    /// This message is send to next hop and relayed to all subscribers except sender.
    Register {
        source: NodeId,
        to_root: bool,
    },
    /// This is used to notify a source is ended.
    /// This message is send to next hop and relayed to all subscribers except sender.
    Unregister {
        source: NodeId,
        to_root: bool,
    },
    Subscribe(u64),
    SubscribeOk(u64),
    Unsubscribe(u64),
    UnsubscribeOk(u64),
    /// This is used when a new subscriber is added, it is like a snapshot for faster initing state.
    Sources(Vec<NodeId>),
}

impl SourceHint {
    pub fn should_create(&self) -> bool {
        matches!(self, SourceHint::Register { .. } | SourceHint::Subscribe(_))
    }
}

pub enum PubsubMessageError {
    // Ask this one is never used. Please give inputs on how to use this.
    // Did not want to simply silence the error
    TransportError(TransportMsgHeaderError),
    DeserializeError,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PubsubMessage {
    Control(RelayId, RelayControl),
    SourceHint(ChannelId, SourceHint),
    Data(RelayId, Vec<u8>),
}

impl TryFrom<&[u8]> for PubsubMessage {
    type Error = PubsubMessageError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let msg = TransportMsg::try_from(value).map_err(PubsubMessageError::TransportError)?;
        msg.get_payload_bincode().map_err(|_| PubsubMessageError::DeserializeError)
    }
}

impl Into<Buffer> for PubsubMessage {
    fn into(self) -> Buffer {
        let header = TransportMsgHeader::build(FEATURE_ID, 0, RouteRule::Direct);
        let msg = TransportMsg::from_payload_bincode(header, &self);
        msg.take()
    }
}

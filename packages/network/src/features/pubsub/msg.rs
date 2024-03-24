use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::simple_pub_type;
use serde::{Deserialize, Serialize};

use crate::base::{TransportMsg, TransportMsgHeader, TransportMsgHeaderError};

use super::FEATURE_ID;

simple_pub_type!(ChannelId, u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayId(pub ChannelId, pub NodeId);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelayControl {
    Sub(u64),
    Unsub(u64),
    SubOK(u64),
    UnsubOK(u64),
    RouteChanged(u64),
}

impl RelayControl {
    pub fn should_create(&self) -> bool {
        match self {
            RelayControl::Sub(_) => true,
            _ => false,
        }
    }
}

pub enum PubsubMessageError {
    TransportError(TransportMsgHeaderError),
    DeserializeError,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PubsubMessage {
    Control(RelayId, RelayControl),
    Data(RelayId, Vec<u8>),
}

impl PubsubMessage {
    pub fn write_to(&self, dest: &mut [u8]) -> Option<usize> {
        let header = TransportMsgHeader::build(FEATURE_ID, 0, RouteRule::Direct);
        let msg = TransportMsg::from_payload_bincode(header, &self);
        let buf = msg.take();
        let len = buf.len();
        dest[..len].copy_from_slice(&buf);
        Some(len)
    }
}

impl TryFrom<&[u8]> for PubsubMessage {
    type Error = PubsubMessageError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let msg = TransportMsg::try_from(value).map_err(PubsubMessageError::TransportError)?;
        msg.get_payload_bincode().map_err(|_| PubsubMessageError::DeserializeError)
    }
}

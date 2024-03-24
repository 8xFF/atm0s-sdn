use atm0s_sdn_identity::NodeId;
use atm0s_sdn_utils::simple_pub_type;
use serde::{Deserialize, Serialize};

use crate::base::TransportMsgHeaderError;

pub const CONTROL_PKT_META: u8 = 0x01;
pub const DATA_PKT_META: u8 = 0x02;

simple_pub_type!(ChannelId, u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayId(pub ChannelId, pub NodeId);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelayControl {
    Sub(u64),
    Unsub(u64),
    SubOK(u64),
    UnsubOK(u64),
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
    WrongMeta,
    TooSmall,
}

pub enum PubsubMessage<'a> {
    Control(RelayId, RelayControl),
    Data(RelayId, &'a [u8]),
}

impl<'a> PubsubMessage<'a> {
    pub fn write_to(&self, dest: &mut [u8]) {
        todo!()
    }
}

impl<'a> TryFrom<&'a [u8]> for PubsubMessage<'a> {
    type Error = PubsubMessageError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        todo!()
        // let header = TransportMsgHeader::try_from(value).map_err(|e| Self::Error::TransportError(e))?;

        // match header.meta {
        //     CONTROL_PKT_META => {
        //         let msg = bincode::DefaultOptions::new().with_limit(1499).deserialize(&value[header.serialize_size()..]).map_err(|_| PubsubMessageError::DeserializeError)?;
        //         Ok(PubsubMessage::Control(msg))
        //     }
        //     DATA_PKT_META => {
        //         if value.len() < header.serialize_size() + 12 {
        //             return Err(Self::Error::TooSmall);
        //         }
        //         let ptr = &value[header.serialize_size()..];
        //         let channel_id = u64::from_be_bytes([ptr[0], ptr[1], ptr[2], ptr[3], ptr[4], ptr[5], ptr[6], ptr[7]]);
        //         let source = u32::from_be_bytes([ptr[8], ptr[9], ptr[10], ptr[11]]);
        //         Ok(PubsubMessage::Data(StreamId(channel_id.into(), source), &ptr[12..]))
        //     }
        //     _ => Err(Self::Error::WrongMeta),
        // }
    }
}

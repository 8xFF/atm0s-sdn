use atm0s_sdn_router::core::RouterSync;

use crate::{
    controller_plane::{ConnectionMsg, FeatureMsg},
    msg::{TransportMsg, TransportMsgHeader},
};

#[derive(Debug, Clone)]
pub enum DataEvent {
    Connection(ConnectionMsg),
    RouterSync(RouterSync),
    Feature(FeatureMsg),
    Network(TransportMsg),
}

impl DataEvent {
    pub fn is_network(buf: &[u8]) -> Option<TransportMsgHeader> {
        if buf[0] == 255 || buf[0] == 254 || buf[0] == 253 {
            return None;
        }
        TransportMsgHeader::from_bytes(buf).ok().map(|a| a.0)
    }
}

impl TryFrom<&[u8]> for DataEvent {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match value[0] {
            255 => bincode::deserialize(&value[1..]).map(DataEvent::Connection).map_err(|_| ()),
            254 => bincode::deserialize(&value[1..]).map(DataEvent::RouterSync).map_err(|_| ()),
            253 => bincode::deserialize(&value[1..]).map(DataEvent::Feature).map_err(|_| ()),
            _ => TransportMsg::from_ref(value).map(DataEvent::Network).map_err(|_| ()),
        }
    }
}

impl Into<Vec<u8>> for DataEvent {
    fn into(self) -> Vec<u8> {
        match self {
            DataEvent::Connection(msg) => {
                //TODO optimize this
                let mut buf = bincode::serialize(&msg).expect("Should serialize control message");
                buf.insert(0, 255);
                buf
            }
            DataEvent::RouterSync(msg) => {
                //TODO optimize this
                let mut buf = bincode::serialize(&msg).expect("Should serialize control message");
                buf.insert(0, 254);
                buf
            }
            DataEvent::Feature(msg) => {
                //TODO optimize this
                let mut buf = bincode::serialize(&msg).expect("Should serialize control message");
                buf.insert(0, 253);
                buf
            }
            DataEvent::Network(msg) => msg.take(),
        }
    }
}

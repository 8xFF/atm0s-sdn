use crate::{
    controller_plane::ControlMsg,
    msg::{TransportMsg, TransportMsgHeader},
};

#[derive(Debug, Clone)]
pub enum DataEvent {
    Control(ControlMsg),
    Network(TransportMsg),
}

impl DataEvent {
    pub fn is_network(buf: &[u8]) -> Option<TransportMsgHeader> {
        if buf[0] == 255 {
            return None;
        }
        TransportMsgHeader::from_bytes(buf).ok().map(|a| a.0)
    }
}

impl TryFrom<&[u8]> for DataEvent {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value[0] == 255 {
            //control
            bincode::deserialize(&value[1..]).map(DataEvent::Control).map_err(|_| ())
        } else {
            TransportMsg::from_ref(value).map(DataEvent::Network).map_err(|_| ())
        }
    }
}

impl Into<Vec<u8>> for DataEvent {
    fn into(self) -> Vec<u8> {
        match self {
            DataEvent::Control(msg) => {
                //TODO optimize this
                let mut buf = bincode::serialize(&msg).expect("Should serialize control message");
                buf.insert(0, 255);
                buf
            }
            DataEvent::Network(msg) => msg.take(),
        }
    }
}

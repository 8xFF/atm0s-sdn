use crate::{controller_plane::ControlMsg, msg::TransportMsg};

#[derive(Debug, Clone)]
pub enum DataEvent {
    Control(ControlMsg),
    Network(TransportMsg),
}

impl TryFrom<&[u8]> for DataEvent {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value[0] == 0 {
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
                buf.insert(0, 0);
                buf
            }
            DataEvent::Network(msg) => msg.take(),
        }
    }
}

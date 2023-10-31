use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

#[derive(Deserialize, Serialize, Copy, Clone)]
pub struct ConnId {
    value: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConnDirection {
    Outgoing,
    Incoming,
}

impl ConnDirection {
    pub fn to_byte(&self) -> u8 {
        match self {
            ConnDirection::Outgoing => 0,
            ConnDirection::Incoming => 1,
        }
    }
}

impl ConnId {
    fn from(protocol: u8, direction: ConnDirection, uuid: u64) -> ConnId {
        let value = uuid << 16 | ((protocol as u64) << 8) | (direction.to_byte() as u64);
        ConnId { value }
    }

    pub fn from_out(protocol: u8, uuid: u64) -> ConnId {
        Self::from(protocol, ConnDirection::Outgoing, uuid)
    }

    pub fn from_in(protocol: u8, uuid: u64) -> ConnId {
        Self::from(protocol, ConnDirection::Incoming, uuid)
    }

    pub fn protocol(&self) -> u8 {
        (self.value >> 8) as u8
    }

    pub fn direction(&self) -> ConnDirection {
        match self.value as u8 {
            0 => ConnDirection::Outgoing,
            _ => ConnDirection::Incoming,
        }
    }

    pub fn uuid(&self) -> u64 {
        self.value >> 16
    }
}

impl std::fmt::Display for ConnId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = format!("Conn({:?},{},{})", self.direction(), self.protocol(), self.uuid());
        std::fmt::Display::fmt(&str, f)
    }
}

impl Debug for ConnId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = format!("Conn({:?},{},{})", self.direction(), self.protocol(), self.uuid());
        Debug::fmt(&str, f)
    }
}

impl PartialEq<Self> for ConnId {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

impl Eq for ConnId {}

impl Hash for ConnId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.value);
    }
}

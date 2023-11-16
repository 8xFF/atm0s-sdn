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
    /// Creates a new `ConnId` from the given protocol, connection direction, and UUID.
    ///
    /// # Arguments
    ///
    /// * `protocol` - A `u8` representing the protocol used for the connection.
    /// * `direction` - A `ConnDirection` enum representing the direction of the connection.
    /// * `uuid` - A `u64` representing the UUID of the connection.
    ///
    /// # Returns
    ///
    /// A new `ConnId` instance.
    fn from(protocol: u8, direction: ConnDirection, uuid: u64) -> ConnId {
        let value = uuid << 16 | ((protocol as u64) << 8) | (direction.to_byte() as u64);
        ConnId { value }
    }

    /// Creates a new outgoing `ConnId` from the given `protocol` and `uuid`.
    ///
    /// # Arguments
    ///
    /// * `protocol` - An unsigned 8-bit integer representing the protocol.
    /// * `uuid` - An unsigned 64-bit integer representing the UUID.
    ///
    /// # Returns
    ///
    /// A new `ConnId` instance.
    pub fn from_out(protocol: u8, uuid: u64) -> ConnId {
        Self::from(protocol, ConnDirection::Outgoing, uuid)
    }

    /// Creates a new incoming `ConnId` from the given `protocol` and `uuid`.
    ///
    /// # Arguments
    ///
    /// * `protocol` - A `u8` representing the protocol.
    /// * `uuid` - A `u64` representing the UUID.
    ///
    /// # Returns
    ///
    /// A new incoming `ConnId` instance.
    ///
    /// # Example
    ///
    /// ```
    /// use p_8xff_sdn_identity::ConnId;
    ///
    /// let conn_id = ConnId::from_in(1, 123456789);
    /// ```
    pub fn from_in(protocol: u8, uuid: u64) -> ConnId {
        Self::from(protocol, ConnDirection::Incoming, uuid)
    }

    /// Returns the protocol of the connection ID.
    pub fn protocol(&self) -> u8 {
        (self.value >> 8) as u8
    }

    /// Returns the direction of the connection.
    pub fn direction(&self) -> ConnDirection {
        match self.value as u8 {
            0 => ConnDirection::Outgoing,
            _ => ConnDirection::Incoming,
        }
    }

    /// Returns the UUID of the connection ID.
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conn_id_from_out() {
        let conn_id = ConnId::from_out(1, 123);
        assert_eq!(conn_id.protocol(), 1);
        assert_eq!(conn_id.direction(), ConnDirection::Outgoing);
        assert_eq!(conn_id.uuid(), 123);
    }

    #[test]
    fn test_conn_id_from_in() {
        let conn_id = ConnId::from_in(2, 456);
        assert_eq!(conn_id.protocol(), 2);
        assert_eq!(conn_id.direction(), ConnDirection::Incoming);
        assert_eq!(conn_id.uuid(), 456);
    }

    #[test]
    fn test_conn_id_display() {
        let conn_id = ConnId::from_out(3, 789);
        assert_eq!(format!("{}", conn_id), "Conn(Outgoing,3,789)");
    }

    #[test]
    fn test_conn_id_debug() {
        let conn_id = ConnId::from_in(4, 101112);
        assert_eq!(format!("{:?}", conn_id), "\"Conn(Incoming,4,101112)\"");
    }

    #[test]
    fn test_conn_id_eq() {
        let conn_id1 = ConnId::from_out(5, 131415);
        let conn_id2 = ConnId::from_out(5, 131415);
        let conn_id3 = ConnId::from_in(5, 131415);
        assert_eq!(conn_id1, conn_id2);
        assert_ne!(conn_id1, conn_id3);
    }

    #[test]
    fn test_conn_id_hash() {
        let conn_id1 = ConnId::from_out(6, 161718);
        let conn_id2 = ConnId::from_out(6, 161718);
        let conn_id3 = ConnId::from_in(6, 161718);
        let mut set = std::collections::HashSet::new();
        set.insert(conn_id1);
        assert!(set.contains(&conn_id1));
        assert!(set.contains(&conn_id2));
        assert!(!set.contains(&conn_id3));
    }
}

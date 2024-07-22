use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};

#[derive(Deserialize, Serialize, Copy, Clone)]
pub struct ConnId {
    protocol: u8,
    direction: ConnDirection,
    session: u64,
}

#[derive(Deserialize, Serialize, Copy, Clone, PartialEq, Eq, Debug, PartialOrd, Ord)]
#[repr(u8)]
pub enum ConnDirection {
    Outgoing = 0,
    Incoming = 1,
}

impl ConnId {
    /// Creates a new `ConnId` from the given protocol, connection direction, and UUID.
    ///
    /// # Arguments
    ///
    /// * `protocol` - A `u8` representing the protocol used for the connection.
    /// * `direction` - A `ConnDirection` enum representing the direction of the connection.
    /// * `session` - A `u64` representing the UUID of the connection.
    ///
    /// # Returns
    ///
    /// A new `ConnId` instance.
    pub fn from_raw(protocol: u8, direction: ConnDirection, session: u64) -> ConnId {
        ConnId { protocol, direction, session }
    }

    /// Creates a new outgoing `ConnId` from the given `protocol` and `session`.
    ///
    /// # Arguments
    ///
    /// * `protocol` - An unsigned 8-bit integer representing the protocol.
    /// * `session` - An unsigned 64-bit integer representing the UUID.
    ///
    /// # Returns
    ///
    /// A new `ConnId` instance.
    pub fn from_out(protocol: u8, session: u64) -> ConnId {
        Self::from_raw(protocol, ConnDirection::Outgoing, session)
    }

    /// Creates a new incoming `ConnId` from the given `protocol` and `session`.
    ///
    /// # Arguments
    ///
    /// * `protocol` - A `u8` representing the protocol.
    /// * `session` - A `u64` representing the UUID.
    ///
    /// # Returns
    ///
    /// A new incoming `ConnId` instance.
    ///
    /// # Example
    ///
    /// ```
    /// use atm0s_sdn_identity::ConnId;
    ///
    /// let conn_id = ConnId::from_in(1, 123456789);
    /// ```
    pub fn from_in(protocol: u8, session: u64) -> ConnId {
        Self::from_raw(protocol, ConnDirection::Incoming, session)
    }

    /// Returns the protocol of the connection ID.
    pub fn protocol(&self) -> u8 {
        self.protocol
    }

    /// Returns the direction of the connection.
    pub fn direction(&self) -> ConnDirection {
        self.direction
    }
    /// Returns `true` if the connection is outgoing, `false` otherwise.
    pub fn is_outgoing(&self) -> bool {
        self.direction() == ConnDirection::Outgoing
    }

    /// Returns the UUID of the connection ID.
    pub fn session(&self) -> u64 {
        self.session
    }
}

impl std::fmt::Display for ConnId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = format!("Conn({:?},{},{})", self.direction(), self.protocol(), self.session());
        std::fmt::Display::fmt(&str, f)
    }
}

impl Debug for ConnId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = format!("Conn({:?},{},{})", self.direction(), self.protocol(), self.session());
        Debug::fmt(&str, f)
    }
}

impl PartialEq<Self> for ConnId {
    fn eq(&self, other: &Self) -> bool {
        //compare both session and direction
        self.session == other.session && self.direction == other.direction
    }
}

impl Eq for ConnId {}

impl Hash for ConnId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u8(self.direction as u8);
        state.write_u64(self.session);
    }
}

impl PartialOrd<Self> for ConnId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        //compare both session and direction
        Some(self.cmp(other))
    }
}

impl Ord for ConnId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        //compare both session and direction
        if self.session == other.session {
            self.direction.cmp(&other.direction)
        } else {
            self.session.cmp(&other.session)
        }
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
        assert_eq!(conn_id.session(), 123);
    }

    #[test]
    fn test_conn_id_from_in() {
        let conn_id = ConnId::from_in(2, 456);
        assert_eq!(conn_id.protocol(), 2);
        assert_eq!(conn_id.direction(), ConnDirection::Incoming);
        assert_eq!(conn_id.session(), 456);
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

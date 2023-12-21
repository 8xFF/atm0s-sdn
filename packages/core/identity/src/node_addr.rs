use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

use crate::node_id::NodeId;
pub use multiaddr::Protocol;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeAddr(NodeId, multiaddr::Multiaddr);

impl NodeAddr {
    pub fn empty(node_id: NodeId) -> Self {
        Self(node_id, multiaddr::Multiaddr::empty())
    }

    pub fn node_id(&self) -> NodeId {
        self.0
    }

    pub fn multiaddr(&self) -> &multiaddr::Multiaddr {
        &self.1
    }

    pub fn from_iter<'a>(node_id: NodeId, iter: impl IntoIterator<Item = Protocol<'a>>) -> Self {
        Self(node_id, multiaddr::Multiaddr::from_iter(iter))
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut buf = self.0.to_be_bytes().to_vec();
        buf.extend(self.1.to_vec());
        buf
    }

    pub fn from_vec(buf: &[u8]) -> Option<Self> {
        let node_id = NodeId::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let multiaddr = multiaddr::Multiaddr::try_from(buf[4..].to_vec()).ok()?;
        Some(Self(node_id, multiaddr))
    }
}

impl FromStr for NodeAddr {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split('@');
        let node_id = split.next().ok_or("Missing NodeId".to_string())?.parse::<NodeId>().map_err(|e| e.to_string())?;
        let multiaddr = split.next().unwrap_or("").parse::<multiaddr::Multiaddr>().map_err(|e| e.to_string())?;
        Ok(Self(node_id, multiaddr))
    }
}

impl Display for NodeAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.1.is_empty() {
            write!(f, "{}", self.0)
        } else {
            write!(f, "{}@{}", self.0, self.1)
        }
    }
}

/// A builder for creating `NodeAddr` instances.
pub struct NodeAddrBuilder {
    node_id: NodeId,
    addr: multiaddr::Multiaddr,
}

impl NodeAddrBuilder {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            addr: multiaddr::Multiaddr::empty(),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Adds a protocol to the node address.
    pub fn add_protocol(&mut self, protocol: Protocol) {
        self.addr.push(protocol);
    }

    /// Get the node address.
    pub fn addr(&self) -> NodeAddr {
        NodeAddr(self.node_id, self.addr.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use multiaddr::Multiaddr;

    #[test]
    fn test_to_from_str() {
        let addr = super::NodeAddr::from_str("1@/ip4/127.0.0.1").unwrap();
        assert_eq!(addr, super::NodeAddr(1, "/ip4/127.0.0.1".parse().unwrap()));
        assert_eq!(addr.to_string(), "1@/ip4/127.0.0.1");
    }

    #[test]
    fn test_empty() {
        let addr = super::NodeAddr::from_str("1").unwrap();
        assert_eq!(addr, super::NodeAddr(1, Multiaddr::empty()));
        assert_eq!(addr, super::NodeAddr::empty(1));
        assert_eq!(addr.to_string(), "1");
    }
}

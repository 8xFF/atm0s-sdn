use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::{RouteRule, ServiceBroadcastLevel};
use bytes::BufMut;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub const DEFAULT_MSG_TTL: u8 = 64;

const ROUTE_RULE_DIRECT: u8 = 0;
const ROUTE_RULE_TO_NODE: u8 = 1;
const ROUTE_RULE_TO_SERVICE: u8 = 2;
const ROUTE_RULE_TO_SERVICES: u8 = 3;
const ROUTE_RULE_TO_KEY: u8 = 4;

#[derive(Debug, Eq, PartialEq)]
pub enum MsgHeaderError {
    InvalidVersion,
    InvalidRoute,
    TooSmall,
}

/// Fixed Header Fields
///
/// ```text
///     0                   1                   2                   3
///     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |V=0|E|N|   R   |      TTL      |  Feature       |     Meta     |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         Route destination (Opt)               |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         FromNodeId (Opt)                      |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// ```
///
/// In there
///
/// - Version (V) : 2 bits (now is 0)
/// - Encrypt (E): 1 bits, If this bit is set, this msg should be encrypted
/// - From Node (N)    : 1 bits, If this bit is set, from node_id will occupy 32 bits in header
/// - Route Type (R): 4 bits
///
///     - 0: Direct : which node received this msg will handle it, no route destionation
///     - 1: ToNode : which node received this msg will route it to node_id
///     - 2: ToService : which node received this msg will route it to service meta
///     - 3: ToKey : which node received this msg will route it to key
///     - .. Not used
///
/// - Ttl (TTL): 8 bits
/// - Feature Id: 8 bits
///
/// - Route destination (Route Destination): 32 bits (if R is not Direct)
///
///     - If route type is ToNode, this field is 32bit node_id
///     - If route type is ToService, this field is 32bit service meta
///     - If route type is ToKey, this field is 32bit key
///
/// - From Node Id: 32 bits (optional if N bit is set)
///

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportMsgHeader {
    pub version: u8,
    pub encrypt: bool,
    pub route: RouteRule,
    pub ttl: u8,
    pub feature: u8,
    pub meta: u8,
    /// Which can be anonymous or specific node
    pub from_node: Option<NodeId>,
}

impl TransportMsgHeader {
    /// Builds a message with the given service_id, route rule.
    pub fn new() -> Self {
        Self {
            version: 0,
            encrypt: false,
            route: RouteRule::Direct,
            ttl: DEFAULT_MSG_TTL,
            feature: 0,
            meta: 0,
            from_node: None,
        }
    }

    pub fn build(feature: u8, meta: u8, route: RouteRule) -> Self {
        Self {
            version: 0,
            encrypt: false,
            route,
            ttl: DEFAULT_MSG_TTL,
            feature,
            meta,
            from_node: None,
        }
    }

    /// Set ttl
    pub fn set_ttl(mut self, ttl: u8) -> Self {
        self.ttl = ttl;
        self
    }

    /// Set secure
    pub fn set_encrypt(mut self, encrypt: bool) -> Self {
        self.encrypt = encrypt;
        self
    }

    /// Set from node
    pub fn set_from_node(mut self, from_node: Option<NodeId>) -> Self {
        self.from_node = from_node;
        self
    }

    /// Set to feature
    pub fn set_feature(mut self, feature: u8) -> Self {
        self.feature = feature;
        self
    }

    /// Set to service_id
    pub fn set_meta(mut self, meta: u8) -> Self {
        self.meta = meta;
        self
    }

    /// Set rule
    pub fn set_route(mut self, route: RouteRule) -> Self {
        self.route = route;
        self
    }

    /// Parses a byte slice into a `Msg` struct and returns the number of bytes read.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A byte slice containing the message to be parsed.
    ///
    /// # Returns
    ///
    /// A tuple containing the parsed `Msg` struct and the number of bytes read.
    ///
    /// # Errors
    ///
    /// Returns a `MsgHeaderError` if the message header is invalid.
    #[allow(unused_assignments)]
    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), MsgHeaderError> {
        if bytes.len() < 4 {
            return Err(MsgHeaderError::TooSmall);
        }
        let version = bytes[0] >> 6; //2 bits
        let e_bit = (bytes[0] >> 5) & 1 == 1; //1 bit
        let n_bit = (bytes[0] >> 4) & 1 == 1; //1 bit
        let route_type = bytes[0] & 15; //4 bits

        if version != 0 {
            return Err(MsgHeaderError::InvalidVersion);
        }

        let ttl = bytes[1];
        let feature = bytes[2];
        let meta = bytes[3];

        let mut ptr = 4;

        let route = match route_type {
            ROUTE_RULE_DIRECT => RouteRule::Direct,
            ROUTE_RULE_TO_NODE => {
                if bytes.len() < ptr + 4 {
                    return Err(MsgHeaderError::TooSmall);
                }
                let rr = RouteRule::ToNode(NodeId::from_be_bytes([bytes[ptr], bytes[ptr + 1], bytes[ptr + 2], bytes[ptr + 3]]));
                ptr += 4;
                rr
            }
            ROUTE_RULE_TO_SERVICE => {
                if bytes.len() < ptr + 4 {
                    return Err(MsgHeaderError::TooSmall);
                }
                let rr = RouteRule::ToService(bytes[ptr]);
                ptr += 4;
                rr
            }
            ROUTE_RULE_TO_SERVICES => {
                if bytes.len() < ptr + 4 {
                    return Err(MsgHeaderError::TooSmall);
                }
                let rr = RouteRule::ToServices(bytes[ptr], ServiceBroadcastLevel::from(bytes[ptr + 1]));
                ptr += 4;
                rr
            }
            ROUTE_RULE_TO_KEY => {
                if bytes.len() < ptr + 4 {
                    return Err(MsgHeaderError::TooSmall);
                }
                let rr = RouteRule::ToKey(NodeId::from_be_bytes([bytes[ptr], bytes[ptr + 1], bytes[ptr + 2], bytes[ptr + 3]]));
                ptr += 4;
                rr
            }
            _ => return Err(MsgHeaderError::InvalidRoute),
        };

        let from_node = if n_bit {
            if bytes.len() < ptr + 4 {
                return Err(MsgHeaderError::TooSmall);
            }
            let from_node_id = NodeId::from_be_bytes([bytes[ptr], bytes[ptr + 1], bytes[ptr + 2], bytes[ptr + 3]]);
            ptr += 4;
            Some(from_node_id)
        } else {
            None
        };

        let len =
            4 + if from_node.is_some() {
                4
            } else {
                0
            } + if route_type == ROUTE_RULE_DIRECT {
                0
            } else {
                4
            };

        Ok((
            Self {
                version,
                encrypt: e_bit,
                ttl,
                route,
                feature,
                meta,
                from_node,
            },
            len,
        ))
    }

    /// Converts the message to a byte representation and appends it to the given output vector.
    ///
    /// # Arguments
    ///
    /// * `output` - A mutable vector of bytes to append the serialized message to.
    ///
    /// # Returns
    ///
    /// An `Option` containing the number of bytes written if the output vector was large enough, or `None` if the output vector was too small.
    #[allow(unused_assignments)]
    pub fn to_bytes(&self, output: &mut [u8]) -> Option<usize> {
        if output.remaining_mut() < self.serialize_size() {
            return None;
        }

        let e_bit = if self.encrypt {
            1 << 5
        } else {
            0
        };
        let n_bit = if self.from_node.is_some() {
            1 << 4
        } else {
            0
        };

        let route_type = match self.route {
            RouteRule::Direct => ROUTE_RULE_DIRECT,
            RouteRule::ToNode(_) => ROUTE_RULE_TO_NODE,
            RouteRule::ToService(_) => ROUTE_RULE_TO_SERVICE,
            RouteRule::ToServices(_, _) => ROUTE_RULE_TO_SERVICES,
            RouteRule::ToKey(_) => ROUTE_RULE_TO_KEY,
        };

        output[0] = (self.version << 6) | e_bit | n_bit | (route_type & 15);
        output[1] = self.ttl;
        output[2] = self.feature;
        output[3] = self.meta;
        let mut ptr = 4;
        match self.route {
            RouteRule::Direct => {
                // Dont need append anything
            }
            RouteRule::ToNode(node_id) => {
                output[ptr..ptr + 4].copy_from_slice(&node_id.to_be_bytes());
                ptr += 4;
            }
            RouteRule::ToService(service) => {
                output[ptr] = service;
                ptr += 4;
            }
            RouteRule::ToServices(service, level) => {
                output[ptr] = service;
                output[ptr + 1] = level.into();
                ptr += 4;
            }
            RouteRule::ToKey(key) => {
                output[ptr..ptr + 4].copy_from_slice(&key.to_be_bytes());
                ptr += 4;
            }
        }
        if let Some(from_node) = self.from_node {
            output[ptr..ptr + 4].copy_from_slice(&from_node.to_be_bytes());
            ptr += 4;
        }

        Some(
            4 + if self.from_node.is_some() {
                4
            } else {
                0
            } + if self.route == RouteRule::Direct {
                0
            } else {
                4
            },
        )
    }

    /// Rewrite the ttl in the given buffer with the new ttl.
    ///
    /// # Arguments
    ///
    /// * `buf` - A mutable slice of bytes representing the buffer to rewrite the ttl in.
    /// * `new_ttl` - The new ttl to use for rewriting the ttl.
    ///
    /// # Returns
    ///
    /// An `Option` containing `()` if the ttl was successfully rewritten,
    /// or `None` if the buffer is too small to hold the new ttl.
    pub fn rewrite_ttl(buf: &mut [u8], new_ttl: u8) -> Option<()> {
        if buf.len() < 2 {
            return None;
        }
        buf[1] = new_ttl;
        Some(())
    }

    /// Decrease the ttl in the given buffer.
    ///
    /// # Arguments
    ///
    /// * `buf` - A mutable slice of bytes representing the buffer to decrease the ttl in.
    ///
    /// # Returns
    ///
    /// An `Option` containing `()` if the ttl was successfully decreased,
    /// or `None` if the buffer is too small to hold the new ttl or ttl too small.
    pub fn decrease_ttl(buf: &mut [u8]) -> Option<()> {
        if buf.len() < 2 {
            return None;
        }
        if buf[1] == 0 {
            return None;
        }
        buf[1] = buf[1].saturating_sub(1);
        Some(())
    }

    /// Returns the size of the serialized message.
    pub fn serialize_size(&self) -> usize {
        4 + if self.from_node.is_some() {
            4
        } else {
            0
        } + if self.route == RouteRule::Direct {
            0
        } else {
            4
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A struct representing a transport message.
pub struct TransportMsg {
    buffer: Vec<u8>,
    pub header: TransportMsgHeader,
    pub payload_start: usize,
}

/// TransportMsg represents a message that can be sent over the network.
/// It contains methods for building, modifying, and extracting data from the message.
impl TransportMsg {
    /// Check if the message is secure
    pub fn is_secure_header(first_byte: u8) -> bool {
        (first_byte >> 2) & 1 == 1
    }

    /// Builds a raw message from a message header and payload.
    ///
    /// # Arguments
    ///
    /// * `header` - The message header.
    /// * `payload` - The message payload.
    ///
    /// # Returns
    ///
    /// A new `TransportMsg` instance.
    pub fn build_raw(header: TransportMsgHeader, payload: &[u8]) -> Self {
        let header_size = header.serialize_size();
        let mut buffer = vec![0; header_size + payload.len()];
        let header_size = header.to_bytes(&mut buffer[0..header_size]).expect("Should serialize header");

        buffer[header_size..].copy_from_slice(payload);
        Self {
            buffer,
            header,
            payload_start: header_size,
        }
    }

    /// Builds a message from a service ID, route rule, stream ID, and payload.
    ///
    /// # Arguments
    ///
    /// * `service_id` - The service ID of the message.
    /// * `route` - The route rule of the message.
    /// * `stream_id` - The stream ID of the message.
    /// * `payload` - The payload of the message.
    ///
    /// # Returns
    ///
    /// A new `TransportMsg` instance.
    pub fn build(feature: u8, meta: u8, route: RouteRule, payload: &[u8]) -> Self {
        let header = TransportMsgHeader::new().set_feature(feature).set_meta(meta).set_route(route);
        let header_size = header.serialize_size();
        let mut buffer = vec![0; header_size + payload.len()];
        let header_size = header.to_bytes(&mut buffer[0..header_size]).expect("Should serialize header");

        buffer[header_size..].copy_from_slice(payload);
        Self {
            buffer,
            header,
            payload_start: header_size,
        }
    }

    /// Takes ownership of the message and returns its buffer.
    pub fn take(self) -> Vec<u8> {
        self.buffer
    }

    /// Constructs a TransportMsg from a vector.
    ///
    /// # Arguments
    ///
    /// * `buf` - A vector of bytes containing the message to be parsed.
    ///
    /// # Returns
    ///
    /// A new `TransportMsg` instance.
    ///
    pub fn from_vec(buffer: Vec<u8>) -> Result<Self, MsgHeaderError> {
        let (header, header_size) = TransportMsgHeader::from_bytes(&buffer)?;
        Ok(Self {
            buffer,
            header,
            payload_start: header_size,
        })
    }

    /// Constructs a TransportMsg from a buf, this will link to buffer.
    ///
    /// # Arguments
    ///
    /// * `buf` - A vector of bytes containing the message to be parsed.
    ///
    /// # Returns
    ///
    /// A new `TransportMsg` instance.
    ///
    pub fn from_ref(buf: &[u8]) -> Result<Self, MsgHeaderError> {
        let (header, header_size) = TransportMsgHeader::from_bytes(buf)?;
        Ok(Self {
            buffer: buf.to_vec(),
            header,
            payload_start: header_size,
        })
    }

    /// Returns a reference to the message buffer.
    pub fn get_buf(&self) -> &[u8] {
        &self.buffer
    }

    /// Returns a reference to the message payload.
    pub fn payload(&self) -> &[u8] {
        &self.buffer[self.payload_start..]
    }

    /// Returns a mutable reference to the message payload.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        &mut self.buffer[self.payload_start..]
    }

    /// Deserializes the message payload into a given type using bincode.
    ///
    /// # Type Parameters
    ///
    /// * `M` - The type to deserialize the payload into.
    ///
    /// # Returns
    ///
    /// A `bincode::Result` containing the deserialized payload.
    pub fn get_payload_bincode<M: DeserializeOwned>(&self) -> bincode::Result<M> {
        bincode::deserialize::<M>(self.payload())
    }

    /// Constructs a TransportMsg from a message header and payload using bincode.
    ///
    /// # Arguments
    ///
    /// * `header` - The message header.
    /// * `msg` - The message payload.
    ///
    /// # Returns
    ///
    /// A new `TransportMsg` instance.
    pub fn from_payload_bincode<M: Serialize>(header: TransportMsgHeader, msg: &M) -> Self {
        let header_size = header.serialize_size();
        let payload_size = bincode::serialized_size(msg).expect("Should serialize payload");
        let mut buffer = vec![0; header_size + payload_size as usize];
        header.to_bytes(&mut buffer[0..header_size]);
        bincode::serialize_into(&mut buffer[header_size..], msg).expect("Should serialize payload");
        Self {
            buffer,
            header,
            payload_start: header_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// test header without option
    #[test]
    fn test_header_without_option() {
        let mut buf = vec![0u8; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 1,
            feature: 2,
            meta: 3,
            route: RouteRule::Direct,
            encrypt: true,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 4);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.serialize_size(), 4);
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 1);
        assert_eq!(header.feature, 2);
        assert_eq!(header.meta, 3);
        assert_eq!(header.route, RouteRule::Direct);
        assert_eq!(header.encrypt, true);
        assert_eq!(header.from_node, None);
    }

    /// test header without option
    #[test]
    fn test_header_with_node_dest() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 1,
            feature: 2,
            meta: 3,
            route: RouteRule::ToNode(4),
            encrypt: true,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 8);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 1);
        assert_eq!(header.feature, 2);
        assert_eq!(header.meta, 3);
        assert_eq!(header.route, RouteRule::ToNode(4));
        assert_eq!(header.from_node, None);
    }

    /// test header without option
    #[test]
    fn test_header_with_service_dest() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 1,
            feature: 2,
            meta: 3,
            route: RouteRule::ToServices(4, ServiceBroadcastLevel::Geo2),
            encrypt: true,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 8);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 1);
        assert_eq!(header.feature, 2);
        assert_eq!(header.meta, 3);
        assert_eq!(header.route, RouteRule::ToServices(4, ServiceBroadcastLevel::Geo2));
        assert_eq!(header.from_node, None);
    }

    /// test header without option
    #[test]
    fn test_header_with_all_options() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 1,
            feature: 2,
            meta: 3,
            route: RouteRule::ToService(4),
            encrypt: true,
            from_node: Some(5),
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 13);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 1);
        assert_eq!(header.feature, 2);
        assert_eq!(header.meta, 3);
        assert_eq!(header.route, RouteRule::ToService(4));
        assert_eq!(header.from_node, Some(5));
    }

    /// test with invalid version
    #[test]
    fn test_with_invalid_version() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 1,
            ttl: 1,
            feature: 2,
            meta: 3,
            route: RouteRule::ToNode(4),
            encrypt: true,
            from_node: Some(5),
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        let err = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap_err();
        assert_eq!(err, MsgHeaderError::InvalidVersion);
    }

    #[test]
    fn msg_simple() {
        let msg = TransportMsg::build(0, 0, RouteRule::Direct, &[1, 2, 3, 4]);
        let buf = msg.get_buf().to_vec();
        let msg2 = TransportMsg::from_vec(buf).unwrap();
        assert_eq!(msg, msg2);
        assert_eq!(msg.payload(), &[1, 2, 3, 4]);
    }

    #[test]
    fn msg_build_raw() {
        let msg = TransportMsg::build_raw(TransportMsgHeader::new(), &[1, 2, 3, 4]);
        let buf = msg.get_buf().to_vec();
        let msg2 = TransportMsg::from_vec(buf).unwrap();
        assert_eq!(msg, msg2);
        assert_eq!(msg.payload(), &[1, 2, 3, 4]);
    }
}

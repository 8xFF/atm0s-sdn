use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use bytes::BufMut;
use serde::de::DeserializeOwned;
use serde::Serialize;

pub const DEFAULT_MSG_TTL: u8 = 64;

const ROUTE_RULE_DIRECT: u8 = 0;
const ROUTE_RULE_TO_NODE: u8 = 1;
const ROUTE_RULE_TO_SERVICE: u8 = 2;
const ROUTE_RULE_TO_KEY: u8 = 3;

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
///    |V=0|F| R |S| M |      TTL      |  FromService   |  ToService   |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         StreamID                              |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         FromNodeId (Opt)                      |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         RouteDestination (Opt)                |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// ```
///
/// In there
///
/// - Version (V) : 2 bits (now is 0)
/// - From (F)    : 1 bits, If this bit is set, from node_id will occupy 32 bits in header
/// - Route Type (R): 2 bits
///
///     - 0: Direct : which node received this msg will handle it, no route destionation
///     - 1: ToNode : which node received this msg will route it to node_id
///     - 2: ToService : which node received this msg will route it to service meta
///     - 3: ToKey : which node received this msg will route it to key
///  - Secure: 1 bits, If this bit is set, this msg should be encrypted
///  - Meta (M): 2 bits
///
/// - Ttl (TTL): 8 bits
/// - From Service Id: 8 bits
/// - To Service Id: 8 bits
/// - Route destination (Route Destination): 32 bits (if R is not Direct)
///
///     - If route type is ToNode, this field is 32bit node_id
///     - If route type is ToService, this field is service meta
///     - If route type is ToKey, this field is 32bit key
///
/// - Stream ID: 32 bits
/// - FromNodeId: 32 bits (optional if F bit is set)
///
///
///

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportMsgHeader {
    pub version: u8,
    pub ttl: u8,
    pub from_service_id: u8,
    pub to_service_id: u8,
    pub route: RouteRule,
    pub secure: bool,
    pub meta: u8,
    pub stream_id: u32,
    /// Which can be anonymous or specific node
    pub from_node: Option<NodeId>,
}

impl TransportMsgHeader {
    /// Builds a message with the given service_id, route rule.
    pub fn new() -> Self {
        Self {
            version: 0,
            ttl: DEFAULT_MSG_TTL,
            from_service_id: 0,
            to_service_id: 0,
            route: RouteRule::Direct,
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: None,
        }
    }

    pub fn build(from_service_id: u8, to_service_id: u8, route: RouteRule) -> Self {
        Self {
            version: 0,
            ttl: DEFAULT_MSG_TTL,
            from_service_id,
            to_service_id,
            route,
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: None,
        }
    }

    /// Set ttl
    pub fn set_ttl(mut self, ttl: u8) -> Self {
        self.ttl = ttl;
        self
    }

    /// Set secure
    pub fn set_secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// Set meta
    pub fn set_meta(mut self, meta: u8) -> Self {
        assert!(meta <= 3);
        self.meta = meta;
        self
    }

    /// Set stream_id
    pub fn set_stream_id(mut self, stream_id: u32) -> Self {
        self.stream_id = stream_id;
        self
    }

    /// Set from node
    pub fn set_from_node(mut self, from_node: Option<NodeId>) -> Self {
        self.from_node = from_node;
        self
    }

    /// Set from service_id
    pub fn set_from_service_id(mut self, service_id: u8) -> Self {
        self.from_service_id = service_id;
        self
    }

    /// Set to service_id
    pub fn set_to_service_id(mut self, service_id: u8) -> Self {
        self.to_service_id = service_id;
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
        if bytes.len() < 8 {
            return Err(MsgHeaderError::TooSmall);
        }
        let version = bytes[0] >> 6; //2 bits
        let from_bit = (bytes[0] >> 5) & 1 == 1; //1 bit
        let route_type = (bytes[0] >> 3) & 3; //2 bits
        let secure = (bytes[0] >> 2) & 1 == 1; //1 bit
        let meta = bytes[0] & 3; //2 bits

        if version != 0 {
            return Err(MsgHeaderError::InvalidVersion);
        }

        let ttl = bytes[1];
        let from_service_id = bytes[2];
        let to_service_id = bytes[3];
        let stream_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        let mut ptr = 8;

        let from_node = if from_bit {
            if bytes.len() < ptr + 4 {
                return Err(MsgHeaderError::TooSmall);
            }
            let from_node_id = NodeId::from_be_bytes([bytes[ptr], bytes[ptr + 1], bytes[ptr + 2], bytes[ptr + 3]]);
            ptr += 4;
            Some(from_node_id)
        } else {
            None
        };

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
                let rr = RouteRule::ToService(u32::from_be_bytes([bytes[ptr], bytes[ptr + 1], bytes[ptr + 2], bytes[ptr + 3]]));
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

        let len =
            8 + if from_node.is_some() {
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
                ttl,
                from_service_id,
                to_service_id,
                route,
                secure,
                meta,
                stream_id,
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
        let route_type = match self.route {
            RouteRule::Direct => ROUTE_RULE_DIRECT,
            RouteRule::ToNode(_) => ROUTE_RULE_TO_NODE,
            RouteRule::ToService(_) => ROUTE_RULE_TO_SERVICE,
            RouteRule::ToKey(_) => ROUTE_RULE_TO_KEY,
        };
        assert!(self.meta <= 3, "meta should <= 3");
        let secure_byte = if self.secure {
            1 << 2
        } else {
            0
        };
        output[0] = (self.version << 6) | ((self.from_node.is_some() as u8) << 5) | route_type << 3 | secure_byte | self.meta;
        output[1] = self.ttl;
        output[2] = self.from_service_id;
        output[3] = self.to_service_id;
        output[4..8].copy_from_slice(&self.stream_id.to_be_bytes());
        let mut ptr = 8;
        if let Some(from_node) = self.from_node {
            output[ptr..ptr + 4].copy_from_slice(&from_node.to_be_bytes());
            ptr += 4;
        }
        match self.route {
            RouteRule::Direct => {
                // Dont need append anything
            }
            RouteRule::ToNode(node_id) => {
                output[ptr..ptr + 4].copy_from_slice(&node_id.to_be_bytes());
                ptr += 4;
            }
            RouteRule::ToService(service_meta) => {
                output[ptr..ptr + 4].copy_from_slice(&service_meta.to_be_bytes());
                ptr += 4;
            }
            RouteRule::ToKey(key) => {
                output[ptr..ptr + 4].copy_from_slice(&key.to_be_bytes());
                ptr += 4;
            }
        }
        Some(
            8 + if self.from_node.is_some() {
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

    /// Rewrite the route in the given buffer with the new route rule.
    ///
    /// # Arguments
    ///
    /// * `buf` - A mutable slice of bytes representing the buffer to rewrite the route in.
    /// * `new_route` - The new route rule to use for rewriting the route.
    ///
    /// # Returns
    ///
    /// An `Option` containing `()` if the route was successfully rewritten, or `None` if the buffer is too small to hold the new route.
    pub fn rewrite_route(buf: &mut [u8], new_route: RouteRule) -> Option<()> {
        if buf.len() < 8 {
            return None;
        }
        match new_route {
            RouteRule::Direct => {
                return None;
            }
            RouteRule::ToNode(node_id) => {
                buf[4..8].copy_from_slice(&node_id.to_be_bytes());
            }
            RouteRule::ToService(service_meta) => {
                buf[4..8].copy_from_slice(&service_meta.to_be_bytes());
            }
            RouteRule::ToKey(key) => {
                buf[4..8].copy_from_slice(&key.to_be_bytes());
            }
        }
        let route_type = match new_route {
            RouteRule::Direct => 0,
            RouteRule::ToNode(_) => 1,
            RouteRule::ToService(_) => 2,
            RouteRule::ToKey(_) => 3,
        };
        buf[0] &= 0b11111000;
        buf[0] |= route_type;
        Some(())
    }

    /// Returns the size of the serialized message.
    pub fn serialize_size(&self) -> usize {
        8 + if self.from_node.is_some() {
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
    pub fn build(from_service_id: u8, to_service_id: u8, route: RouteRule, meta: u8, stream_id: u32, payload: &[u8]) -> Self {
        let header = TransportMsgHeader::new()
            .set_from_service_id(from_service_id)
            .set_to_service_id(to_service_id)
            .set_route(route)
            .set_meta(meta)
            .set_stream_id(stream_id);
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

    /// Rewrites the route rule in the message header.
    ///
    /// # Arguments
    ///
    /// * `new_route` - The new route rule to use for rewriting the route.
    ///
    /// # Returns
    ///
    /// An `Option` containing `()` if the route was successfully rewritten, or `None` if the buffer is too small to hold the new route.
    pub fn rewrite_route(&mut self, new_route: RouteRule) -> Option<()> {
        TransportMsgHeader::rewrite_route(&mut self.buffer, new_route)
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
            ttl: 0,
            from_service_id: 0,
            to_service_id: 0,
            route: RouteRule::Direct,
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 8);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.serialize_size(), 8);
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.from_service_id, 0);
        assert_eq!(header.to_service_id, 0);
        assert_eq!(header.route, RouteRule::Direct);
        assert_eq!(header.secure, true);
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, None);
    }

    /// test header without option2
    #[test]
    fn test_header_without_option2() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 10,
            from_service_id: 88,
            to_service_id: 88,
            route: RouteRule::Direct,
            secure: true,
            meta: 0,
            stream_id: 1234,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 8);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 10);
        assert_eq!(header.from_service_id, 88);
        assert_eq!(header.to_service_id, 88);
        assert_eq!(header.route, RouteRule::Direct);
        assert_eq!(header.secure, true);
        assert_eq!(header.stream_id, 1234);
        assert_eq!(header.from_node, None);
    }

    /// test header without option
    #[test]
    fn test_header_with_dest2() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 0,
            from_service_id: 0,
            to_service_id: 0,
            route: RouteRule::ToNode(0),
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 12);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.from_service_id, 0);
        assert_eq!(header.to_service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, None);
    }

    /// test header without option
    #[test]
    fn test_header_with_all_options() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 66,
            from_service_id: 33,
            to_service_id: 34,
            route: RouteRule::ToNode(111),
            secure: true,
            meta: 3,
            stream_id: 222,
            from_node: Some(1000),
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        println!("{:?}", buf);
        assert_eq!(header.serialize_size(), 16);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 66);
        assert_eq!(header.from_service_id, 33);
        assert_eq!(header.to_service_id, 34);
        assert_eq!(header.route, RouteRule::ToNode(111));
        assert_eq!(header.secure, true);
        assert_eq!(header.meta, 3);
        assert_eq!(header.stream_id, 222);
        assert_eq!(header.from_node, Some(1000));
    }

    /// test header with router dest
    #[test]
    fn test_header_with_dest() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 10,
            from_service_id: 88,
            to_service_id: 88,
            route: RouteRule::ToNode(1000),
            secure: true,
            meta: 0,
            stream_id: 1234,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 12);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 10);
        assert_eq!(header.from_service_id, 88);
        assert_eq!(header.to_service_id, 88);
        assert_eq!(header.route, RouteRule::ToNode(1000));
        assert_eq!(header.secure, true);
        assert_eq!(header.stream_id, 1234);
        assert_eq!(header.from_node, None);
    }

    /// test header with option: from_node
    #[test]
    fn test_header_with_option_from_node() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 0,
            from_service_id: 0,
            to_service_id: 0,
            route: RouteRule::ToNode(0),
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: Some(0),
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 16);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.from_service_id, 0);
        assert_eq!(header.to_service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.secure, true);
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, Some(0));
    }

    /// test header with option: validate_code
    #[test]
    fn test_header_with_option_validate_code() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 0,
            from_service_id: 0,
            to_service_id: 0,
            route: RouteRule::ToNode(0),
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 12);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.from_service_id, 0);
        assert_eq!(header.to_service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.secure, true);
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, None);
    }

    /// test header with option: from_node and validate_code
    #[test]
    fn test_header_with_option_from_node_and_validate_code() {
        let mut buf = [0; 20];
        let header = TransportMsgHeader {
            version: 0,
            ttl: 0,
            from_service_id: 0,
            to_service_id: 0,
            route: RouteRule::ToNode(0),
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: Some(0),
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        assert_eq!(header.serialize_size(), 16);
        let (header, _) = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.from_service_id, 0);
        assert_eq!(header.to_service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.secure, true);
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, Some(0));
    }

    /// test with invalid version
    #[test]
    fn test_with_invalid_version() {
        let mut buf = [0; 16];
        let header = TransportMsgHeader {
            version: 1,
            ttl: 0,
            from_service_id: 0,
            to_service_id: 0,
            route: RouteRule::ToNode(0),
            secure: true,
            meta: 0,
            stream_id: 0,
            from_node: None,
        };
        let size = header.to_bytes(&mut buf).expect("should serialize");
        let err = TransportMsgHeader::from_bytes(&buf[0..size]).unwrap_err();
        assert_eq!(err, MsgHeaderError::InvalidVersion);
    }

    #[test]
    fn msg_simple() {
        let msg = TransportMsg::build(0, 0, RouteRule::Direct, 0, 0, &[1, 2, 3, 4]);
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

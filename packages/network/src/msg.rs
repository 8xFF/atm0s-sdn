use bytes::BufMut;
use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
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
///    |V=0|R|F|V|  S |      TTL      |    Service     |      Meta     |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         Stream ID                             |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         Route Destination (Option)            |
///    +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
///    |                         FromNodeId (Option)                   |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///    |                         ValidateCode (Option)                 |
///    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// ```
///
/// In there
///
/// - version (V) : 2 bits (now is 0)
/// - reliable (R): 1 bits
/// - from (F)    : 1 bits, If this bit is set, from node_id will occupy 32 bits in header
/// - validation (V): 1 bits
/// - route type (S): 3 bits
///
///     - 0: Direct : which node received this msg will handle it, no route destionation
///     - 1: ToNode : which node received this msg will route it to node_id
///     - 2: ToService : which node received this msg will route it to service meta
///     - 3: ToKey : which node received this msg will route it to key
///     - 4-7: Reserved
/// - ttl (TTL): 8 bits
/// - service id (Service): 8 bits
/// - meta (M): 8 bits (can use freely)
/// - route destination (Route Destination): 32 bits (if S is not Direct)
///
///     - If route type is ToNode, this field is 32bit node_id
///     - If route type is ToService, this field is service meta
///     - If route type is ToKey, this field is 32bit key
/// - stream id (Stream ID): 32 bits
/// - from_id (FromNodeId): 32 bits (optional if F bit is set)
/// - validate_code (ValidateCode): 32 bits (optional if V bit is set)
///
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MsgHeader {
    pub version: u8,
    pub reliable: bool,
    pub ttl: u8,
    pub service_id: u8,
    pub route: RouteRule,
    pub meta: u8,
    pub stream_id: u32,
    /// Which can be anonymous or specific node
    pub from_node: Option<NodeId>,
    pub validate_code: Option<u32>,
}

impl MsgHeader {
    /// Builds a reliable message with the given service ID, route rule, and stream ID.
    pub fn build_reliable(service_id: u8, route: RouteRule, stream_id: u32) -> Self {
        Self {
            version: 0,
            reliable: true,
            ttl: DEFAULT_MSG_TTL,
            service_id,
            route,
            meta: 0,
            stream_id,
            from_node: None,
            validate_code: None,
        }
    }

    /// Builds an unreliable message with the given service ID, route rule, and stream ID.
    pub fn build_unreliable(service_id: u8, route: RouteRule, stream_id: u32) -> Self {
        Self {
            version: 0,
            reliable: false,
            ttl: DEFAULT_MSG_TTL,
            service_id,
            route,
            meta: 0,
            stream_id,
            from_node: None,
            validate_code: None,
        }
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
    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), MsgHeaderError> {
        if bytes.len() < 8 {
            return Err(MsgHeaderError::TooSmall);
        }
        let version = bytes[0] >> 6;
        let reliable = (bytes[0] >> 5) & 1 == 1;
        let from_bit = (bytes[0] >> 4) & 1 == 1;
        let validate_bit = (bytes[0] >> 3) & 1 == 1;
        let route_type = bytes[0] & 7;

        if version != 0 {
            return Err(MsgHeaderError::InvalidVersion);
        }

        let ttl = bytes[1];
        let service_id = bytes[2];
        let meta = bytes[3];
        let stream_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        let mut ptr = 8;

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

        let validate_code = if validate_bit {
            if bytes.len() < ptr + 4 {
                return Err(MsgHeaderError::TooSmall);
            }
            Some(u32::from_be_bytes([bytes[ptr], bytes[ptr + 1], bytes[ptr + 2], bytes[ptr + 3]]))
        } else {
            None
        };
        Ok((
            Self {
                version,
                reliable,
                ttl,
                service_id,
                route,
                meta,
                stream_id,
                from_node,
                validate_code,
            },
            8 + if from_node.is_some() {
                4
            } else {
                0
            } + if validate_code.is_some() {
                4
            } else {
                0
            } + if route_type == ROUTE_RULE_DIRECT {
                0
            } else {
                4
            },
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
    pub fn to_bytes(&self, output: &mut Vec<u8>) -> Option<usize> {
        if output.remaining_mut() < self.serialize_size() {
            return None;
        }
        let route_type = match self.route {
            RouteRule::Direct => ROUTE_RULE_DIRECT,
            RouteRule::ToNode(_) => ROUTE_RULE_TO_NODE,
            RouteRule::ToService(_) => ROUTE_RULE_TO_SERVICE,
            RouteRule::ToKey(_) => ROUTE_RULE_TO_KEY,
        };
        output.push((self.version << 6) | ((self.reliable as u8) << 5) | ((self.from_node.is_some() as u8) << 4) | ((self.validate_code.is_some() as u8) << 3) | route_type);
        output.push(self.ttl);
        output.push(self.service_id);
        output.push(self.meta);
        output.extend_from_slice(&self.stream_id.to_be_bytes());
        match self.route {
            RouteRule::Direct => {
                // Dont need append anything
            }
            RouteRule::ToNode(node_id) => {
                output.extend_from_slice(&node_id.to_be_bytes());
            }
            RouteRule::ToService(service_meta) => {
                output.extend_from_slice(&service_meta.to_be_bytes());
            }
            RouteRule::ToKey(key) => {
                output.extend_from_slice(&key.to_be_bytes());
            }
        }
        if let Some(from_node) = self.from_node {
            output.extend_from_slice(&from_node.to_be_bytes());
        }
        if let Some(validate_code) = self.validate_code {
            output.extend_from_slice(&validate_code.to_be_bytes());
        }
        Some(
            8 + if self.from_node.is_some() {
                4
            } else {
                0
            } + if self.validate_code.is_some() {
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
        } + if self.validate_code.is_some() {
            4
        } else {
            0
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A struct representing a transport message.
pub struct TransportMsg {
    buffer: Vec<u8>,
    pub header: MsgHeader,
    pub payload_start: usize,
}

/// TransportMsg represents a message that can be sent over the network.
/// It contains methods for building, modifying, and extracting data from the message.
impl TransportMsg {
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
    pub fn build_raw(header: MsgHeader, payload: &[u8]) -> Self {
        let header_size = header.serialize_size();
        let mut buffer = Vec::with_capacity(header_size + payload.len());
        let header_size = header.to_bytes(&mut buffer).expect("Should serialize header");

        buffer.extend_from_slice(payload);
        Self {
            buffer,
            header,
            payload_start: header_size,
        }
    }

    /// Builds a reliable message from a service ID, route rule, stream ID, and payload.
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
    pub fn build_reliable(service_id: u8, route: RouteRule, stream_id: u32, payload: &[u8]) -> Self {
        let header = MsgHeader::build_reliable(service_id, route, stream_id);
        let header_size = header.serialize_size();
        let mut buffer = Vec::with_capacity(header_size + payload.len());
        let header_size = header.to_bytes(&mut buffer).expect("Should serialize header");

        buffer.extend_from_slice(payload);
        Self {
            buffer,
            header,
            payload_start: header_size,
        }
    }

    /// Builds an unreliable message from a service ID, route rule, stream ID, and payload.
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
    pub fn build_unreliable(service_id: u8, route: RouteRule, stream_id: u32, payload: &[u8]) -> Self {
        let header = MsgHeader::build_unreliable(service_id, route, stream_id);
        let header_size = header.serialize_size();
        let mut buffer = Vec::with_capacity(header_size + payload.len());
        header.to_bytes(&mut buffer);
        buffer.extend_from_slice(payload);
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

    /// Constructs a TransportMsg from a buffer.
    ///
    /// # Arguments
    ///
    /// * `buf` - A vector of bytes containing the message to be parsed.
    ///
    /// # Returns
    ///
    /// A new `TransportMsg` instance.
    ///
    pub fn from_vec(buf: Vec<u8>) -> Result<Self, MsgHeaderError> {
        let (header, header_size) = MsgHeader::from_bytes(&buf)?;
        Ok(Self {
            buffer: buf,
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
        MsgHeader::rewrite_route(&mut self.buffer, new_route)
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
    pub fn from_payload_bincode<M: Serialize>(header: MsgHeader, msg: &M) -> Self {
        let header_size = header.serialize_size();
        let payload_size = bincode::serialized_size(msg).expect("Should serialize payload");
        let mut buffer = Vec::with_capacity(header_size + payload_size as usize);
        header.to_bytes(&mut buffer);
        bincode::serialize_into(&mut buffer, msg).expect("Should serialize payload");
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
        let header = MsgHeader {
            version: 0,
            reliable: false,
            ttl: 0,
            service_id: 0,
            route: RouteRule::Direct,
            meta: 0,
            stream_id: 0,
            from_node: None,
            validate_code: None,
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 8);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.serialize_size(), 8);
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, false);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.service_id, 0);
        assert_eq!(header.route, RouteRule::Direct);
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, None);
        assert_eq!(header.validate_code, None);
    }

    /// test header without option2
    #[test]
    fn test_header_without_option2() {
        let mut buf = Vec::with_capacity(16);
        let header = MsgHeader {
            version: 0,
            reliable: true,
            ttl: 10,
            service_id: 88,
            route: RouteRule::Direct,
            meta: 0,
            stream_id: 1234,
            from_node: None,
            validate_code: None,
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 8);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, true);
        assert_eq!(header.ttl, 10);
        assert_eq!(header.service_id, 88);
        assert_eq!(header.route, RouteRule::Direct);
        assert_eq!(header.stream_id, 1234);
        assert_eq!(header.from_node, None);
        assert_eq!(header.validate_code, None);
    }

    /// test header without option
    #[test]
    fn test_header_with_dest2() {
        let mut buf = Vec::with_capacity(16);
        let header = MsgHeader {
            version: 0,
            reliable: false,
            ttl: 0,
            service_id: 0,
            route: RouteRule::ToNode(0),
            meta: 0,
            stream_id: 0,
            from_node: None,
            validate_code: None,
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 12);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, false);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, None);
        assert_eq!(header.validate_code, None);
    }

    /// test header without option
    #[test]
    fn test_header_with_all_options() {
        let mut buf = Vec::with_capacity(16);
        let header = MsgHeader {
            version: 0,
            reliable: true,
            ttl: 66,
            service_id: 33,
            route: RouteRule::ToNode(111),
            meta: 55,
            stream_id: 222,
            from_node: Some(1000),
            validate_code: Some(1000),
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 20);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, true);
        assert_eq!(header.ttl, 66);
        assert_eq!(header.service_id, 33);
        assert_eq!(header.route, RouteRule::ToNode(111));
        assert_eq!(header.meta, 55);
        assert_eq!(header.stream_id, 222);
        assert_eq!(header.from_node, Some(1000));
        assert_eq!(header.validate_code, Some(1000));
    }

    /// test header with router dest
    #[test]
    fn test_header_with_dest() {
        let mut buf = Vec::with_capacity(16);
        let header = MsgHeader {
            version: 0,
            reliable: true,
            ttl: 10,
            service_id: 88,
            route: RouteRule::ToNode(1000),
            meta: 0,
            stream_id: 1234,
            from_node: None,
            validate_code: None,
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 12);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, true);
        assert_eq!(header.ttl, 10);
        assert_eq!(header.service_id, 88);
        assert_eq!(header.route, RouteRule::ToNode(1000));
        assert_eq!(header.stream_id, 1234);
        assert_eq!(header.from_node, None);
        assert_eq!(header.validate_code, None);
    }

    /// test header with option: from_node
    #[test]
    fn test_header_with_option_from_node() {
        let mut buf = Vec::with_capacity(16);
        let header = MsgHeader {
            version: 0,
            reliable: false,
            ttl: 0,
            service_id: 0,
            route: RouteRule::ToNode(0),
            meta: 0,
            stream_id: 0,
            from_node: Some(0),
            validate_code: None,
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 16);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, false);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, Some(0));
        assert_eq!(header.validate_code, None);
    }

    /// test header with option: validate_code
    #[test]
    fn test_header_with_option_validate_code() {
        let mut buf = Vec::with_capacity(16);
        let header = MsgHeader {
            version: 0,
            reliable: false,
            ttl: 0,
            service_id: 0,
            route: RouteRule::ToNode(0),
            meta: 0,
            stream_id: 0,
            from_node: None,
            validate_code: Some(0),
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 16);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, false);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, None);
        assert_eq!(header.validate_code, Some(0));
    }

    /// test header with option: from_node and validate_code
    #[test]
    fn test_header_with_option_from_node_and_validate_code() {
        let mut buf = Vec::with_capacity(20);
        let header = MsgHeader {
            version: 0,
            reliable: false,
            ttl: 0,
            service_id: 0,
            route: RouteRule::ToNode(0),
            meta: 0,
            stream_id: 0,
            from_node: Some(0),
            validate_code: Some(0),
        };
        header.to_bytes(&mut buf);
        assert_eq!(header.serialize_size(), 20);
        let (header, _) = MsgHeader::from_bytes(&buf).unwrap();
        assert_eq!(header.version, 0);
        assert_eq!(header.reliable, false);
        assert_eq!(header.ttl, 0);
        assert_eq!(header.service_id, 0);
        assert_eq!(header.route, RouteRule::ToNode(0));
        assert_eq!(header.stream_id, 0);
        assert_eq!(header.from_node, Some(0));
        assert_eq!(header.validate_code, Some(0));
    }

    /// test with invalid version
    #[test]
    fn test_with_invalid_version() {
        let mut buf = Vec::with_capacity(16);
        let header = MsgHeader {
            version: 1,
            reliable: false,
            ttl: 0,
            service_id: 0,
            route: RouteRule::ToNode(0),
            meta: 0,
            stream_id: 0,
            from_node: None,
            validate_code: None,
        };
        header.to_bytes(&mut buf);
        let err = MsgHeader::from_bytes(&buf).unwrap_err();
        assert_eq!(err, MsgHeaderError::InvalidVersion);
    }

    /// test with invalid route
    #[test]
    fn test_with_invalid_route() {
        //this is bytes of invalid route header
        let buf = [0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let err = MsgHeader::from_bytes(&buf).unwrap_err();
        assert_eq!(err, MsgHeaderError::InvalidRoute);
    }
}

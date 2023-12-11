//! Rpc Reliable will use stream_id and meta header of TransportMsg for embeding big message partition info in bellow mechanism
//!
//! - First, each msg is assign with an incremental id u16 in sender node, by that way we can using pair (node_id, msg_id) for identify which message.
//! if we sending more than 2^16 msg in a TIMEOUT WINDOW (3000ms), so we will have overlay (~209 Mbps)   
//! - Second, big msg payload is split into 1200 bytes, and part index and part cound is embded into remain part of stream_id
//!
//! ```text
//!     0                   1                   2                   3
//!     0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//!    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//!    |           MsgId (Opt)        | PartIndex    | PartCount-1     |
//!    +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```
//!
//! With meta value:
//!
//! - 0: Ack
//! - 1: Data

pub(crate) mod msg;
pub(crate) mod recv;
pub(crate) mod send;

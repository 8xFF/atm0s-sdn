mod connection_receiver;
mod connection_sender;
mod transport;

use crate::transport::{
    ConnectionMsg, ConnectionSender, OutgoingConnectionError, Transport, TransportConnector,
    TransportEvent, TransportPendingOutgoing,
};
use kanal::{Receiver, Sender, bounded, unbounded};
use bluesea_identity::{PeerAddr, PeerId};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub enum MockInput<M> {
    FakeIncomingConnection(PeerId, u32, PeerAddr),
    FakeIncomingMsg(u8, u32, ConnectionMsg<M>),
    FakeDisconnectIncoming(PeerId, u32),
}

#[derive(PartialEq, Debug)]
pub enum MockOutput<M> {
    ConnectTo(PeerId, PeerAddr),
    SendTo(u8, PeerId, u32, ConnectionMsg<M>),
}

pub use transport::MockTransport;

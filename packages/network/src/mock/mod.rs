mod connection_sender;
mod connection_receiver;
mod transport;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use async_std::channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use bluesea_identity::{PeerAddr, PeerId};
use crate::transport::{ConnectionMsg, ConnectionSender, OutgoingConnectionError, Transport, TransportConnector, TransportEvent, TransportPendingOutgoing};

pub enum MockInput<M> {
    FakeIncomingConnection(PeerId, u32, PeerAddr),
    FakeIncomingMsg(u8, u32, ConnectionMsg<M>),
}

#[derive(PartialEq, Debug)]
pub enum MockOutput<M> {
    ConnectTo(PeerId, PeerAddr),
    SendTo(u8, PeerId, u32, ConnectionMsg<M>),
}

pub use transport::MockTransport;
mod connection_sender;
mod connection_receiver;
mod transport;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use async_std::channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use bluesea_identity::{PeerAddr, PeerId};
use crate::transport::{ConnectionSender, OutgoingConnectionError, Transport, TransportConnector, TransportEvent, TransportPendingOutgoing};

pub enum MockInput<M> {
    FakeIncomingConnection(PeerId, u32, PeerAddr),
    FakeIncomingMsg(PeerId, u32, M),
}

pub enum MockOutput<M> {
    ConnectTo(PeerId, PeerAddr),
    SendTo(PeerId, u32, M),
}

pub use transport::MockTransport;
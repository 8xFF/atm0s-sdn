mod connection_receiver;
mod connection_sender;
mod transport;
mod transport_rpc;

use crate::transport::{
    ConnectionMsg, ConnectionSender, OutgoingConnectionError, Transport, TransportConnector,
    TransportEvent, TransportPendingOutgoing,
};
use bluesea_identity::{NodeAddr, NodeId};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub enum MockInput<M> {
    FakeIncomingConnection(NodeId, u32, NodeAddr),
    ///Dont use this manual
    FakeIncomingConnectionForce(NodeId, u32, NodeAddr),
    FakeOutgoingConnection(NodeId, u32, NodeAddr),
    ///Dont use this manual
    FakeOutgoingConnectionForce(NodeId, u32, NodeAddr),
    FakeOutgoingConnectionError(NodeId, u32, OutgoingConnectionError),
    FakeIncomingMsg(u8, u32, ConnectionMsg<M>),
    FakeDisconnectIncoming(NodeId, u32),
    FakeDisconnectOutgoing(NodeId, u32),
}

#[derive(PartialEq, Debug)]
pub enum MockOutput<M> {
    ConnectTo(NodeId, NodeAddr),
    SendTo(u8, NodeId, u32, ConnectionMsg<M>),
}

pub use transport::MockTransport;
pub use transport_rpc::MockTransportRpc;

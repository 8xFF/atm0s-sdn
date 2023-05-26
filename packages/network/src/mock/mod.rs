mod connection_receiver;
mod connection_sender;
mod transport;
mod transport_rpc;

use crate::transport::{
    ConnectionMsg, ConnectionSender, OutgoingConnectionError, Transport, TransportConnector,
    TransportEvent, TransportPendingOutgoing,
};
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub enum MockInput<M> {
    FakeIncomingConnection(NodeId, ConnId, NodeAddr),
    ///Dont use this manual
    FakeIncomingConnectionForce(NodeId, ConnId, NodeAddr),
    FakeOutgoingConnection(NodeId, ConnId, NodeAddr),
    ///Dont use this manual
    FakeOutgoingConnectionForce(NodeId, ConnId, NodeAddr),
    FakeOutgoingConnectionError(NodeId, ConnId, OutgoingConnectionError),
    FakeIncomingMsg(u8, ConnId, ConnectionMsg<M>),
    FakeDisconnectIncoming(NodeId, ConnId),
    FakeDisconnectOutgoing(NodeId, ConnId),
}

#[derive(PartialEq, Debug)]
pub enum MockOutput<M> {
    ConnectTo(NodeId, NodeAddr),
    SendTo(u8, NodeId, ConnId, ConnectionMsg<M>),
}

pub use transport::MockTransport;
pub use transport_rpc::MockTransportRpc;

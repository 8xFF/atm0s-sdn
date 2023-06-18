mod connection_receiver;
mod connection_sender;
mod transport;
mod transport_rpc;

use std::sync::Arc;
use std::collections::{HashMap, VecDeque};
use bluesea_identity::{ConnId, NodeAddr, NodeId};
pub use transport::MockTransport;
pub use transport_rpc::MockTransportRpc;
use crate::msg::TransportMsg;
use crate::transport::{ConnectionSender, OutgoingConnectionError, Transport, TransportConnector, TransportEvent, TransportConnectingOutgoing};


pub enum MockInput {
    FakeIncomingConnection(NodeId, ConnId, NodeAddr),
    ///Dont use this manual
    FakeIncomingConnectionForce(NodeId, ConnId, NodeAddr),
    FakeOutgoingConnection(NodeId, ConnId, NodeAddr),
    ///Dont use this manual
    FakeOutgoingConnectionForce(NodeId, ConnId, NodeAddr),
    FakeOutgoingConnectionError(NodeId, ConnId, OutgoingConnectionError),
    FakeIncomingMsg(ConnId, TransportMsg),
    FakeDisconnectIncoming(NodeId, ConnId),
    FakeDisconnectOutgoing(NodeId, ConnId),
}

#[derive(PartialEq, Debug)]
pub enum MockOutput {
    ConnectTo(NodeId, NodeAddr),
    SendTo(NodeId, ConnId, TransportMsg),
}
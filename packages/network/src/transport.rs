use crate::msg::TransportMsg;
use async_std::channel::{bounded, Receiver, Sender};
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use std::sync::Arc;
use thiserror::Error;

#[cfg(test)]
use mockall::automock;

/// Enum representing events that can occur in the transport layer.
///
///
//
///
///
pub enum TransportEvent {
    /// `IncomingRequest` represents an incoming request from a node with the given `NodeId` and `ConnId`,
    /// which is accepted by the `ConnectionAcceptor` trait object.
    IncomingRequest(NodeId, ConnId, Box<dyn ConnectionAcceptor>),
    /// `Incoming` represents an incoming connection with the given `ConnectionSender` and `ConnectionReceiver`.
    Incoming(Arc<dyn ConnectionSender>, Box<dyn ConnectionReceiver + Send>),
    /// `Outgoing` represents an outgoing connection with the given `ConnectionSender`, `ConnectionReceiver`,
    Outgoing(Arc<dyn ConnectionSender>, Box<dyn ConnectionReceiver + Send>),
    /// `OutgoingError` represents an error that occurred while attempting to establish an outgoing connection,
    /// with the given `NodeId`, `ConnId`, and `OutgoingConnectionError`.
    OutgoingError { node_id: NodeId, conn_id: ConnId, err: OutgoingConnectionError },
}

#[async_trait::async_trait]
pub trait Transport: Send {
    fn connector(&mut self) -> &mut dyn TransportConnector;
    async fn recv(&mut self) -> Result<TransportEvent, ()>;
}

/// Transport connector will connect to the given node with bellow lifecycle:
/// 1. create_pending_outgoing
/// 2. wait continue_pending_outgoing or destroy_pending_outgoing
pub trait TransportConnector: Send + Sync {
    /// Trying to spawn all posible connections for the given node. Return the local uuids of the connections.
    /// Note that the connections are not established yet. The connections will be established after called continue_pending_outgoing or destroy with destroy_pending_outgoing.
    fn create_pending_outgoing(&mut self, dest: NodeAddr) -> Vec<ConnId>;
    fn continue_pending_outgoing(&mut self, conn_id: ConnId);
    fn destroy_pending_outgoing(&mut self, conn_id: ConnId);
}

#[derive(PartialEq, Debug, Clone)]
pub struct ConnectionStats {
    pub rtt_ms: u16,
    pub sending_kbps: u32,
    pub send_est_kbps: u32,
    pub loss_percent: u32,
    pub over_use: bool,
}

#[derive(PartialEq, Debug, Clone)]
pub enum ConnectionEvent {
    Msg(TransportMsg),
    Stats(ConnectionStats),
}

#[derive(PartialEq, Error, Clone, Debug)]
pub enum ConnectionRejectReason {
    #[error("Connection Limited")]
    ConnectionLimited,
    #[error("Validate Error")]
    ValidateError,
    #[error("Custom {0}")]
    Custom(String),
}

#[cfg_attr(test, automock)]
pub trait ConnectionAcceptor: Send + Sync {
    fn accept(&self);
    fn reject(&self, err: ConnectionRejectReason);
}

#[cfg_attr(test, automock)]
pub trait ConnectionSender: Send + Sync {
    fn remote_node_id(&self) -> NodeId;
    fn conn_id(&self) -> ConnId;
    fn remote_addr(&self) -> NodeAddr;
    fn send(&self, msg: TransportMsg);
    fn close(&self);
}

#[async_trait::async_trait]
#[cfg_attr(test, automock)]
pub trait ConnectionReceiver {
    fn remote_node_id(&self) -> NodeId;
    fn conn_id(&self) -> ConnId;
    fn remote_addr(&self) -> NodeAddr;
    async fn poll(&mut self) -> Result<ConnectionEvent, ()>;
}

#[derive(PartialEq, Error, Clone, Debug)]
pub enum OutgoingConnectionError {
    #[error("Too many connection")]
    TooManyConnection,
    #[error("Authentication Error")]
    AuthenticationError,
    #[error("Unsupported Protocol")]
    UnsupportedProtocol,
    #[error("Destination Not Found")]
    DestinationNotFound,
    #[error("Behavior Rejected")]
    BehaviorRejected(ConnectionRejectReason),
}

pub struct AsyncConnectionAcceptor {
    sender: Sender<Result<(), ConnectionRejectReason>>,
}

impl AsyncConnectionAcceptor {
    pub fn new() -> (Box<Self>, Receiver<Result<(), ConnectionRejectReason>>) {
        let (sender, receiver) = bounded(1);
        (Box::new(Self { sender }), receiver)
    }
}

impl ConnectionAcceptor for AsyncConnectionAcceptor {
    fn accept(&self) {
        self.sender.send_blocking(Ok(())).print_error("Should send accept");
    }

    fn reject(&self, err: ConnectionRejectReason) {
        self.sender.send_blocking(Err(err)).print_error("Should send reject");
    }
}

use async_std::channel::{bounded, Receiver, Sender};
use bluesea_identity::{NodeAddr, NodeId};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;

pub struct TransportPendingOutgoing {
    pub connection_id: u32,
}

pub enum TransportEvent<MSG> {
    IncomingRequest(NodeId, u32, Box<dyn ConnectionAcceptor>),
    OutgoingRequest(NodeId, u32, Box<dyn ConnectionAcceptor>),
    Incoming(
        Arc<dyn ConnectionSender<MSG>>,
        Box<dyn ConnectionReceiver<MSG> + Send>,
    ),
    Outgoing(
        Arc<dyn ConnectionSender<MSG>>,
        Box<dyn ConnectionReceiver<MSG> + Send>,
    ),
    OutgoingError {
        node_id: NodeId,
        connection_id: u32,
        err: OutgoingConnectionError,
    },
}

#[async_trait::async_trait]
pub trait Transport<MSG> {
    fn connector(&self) -> Arc<dyn TransportConnector>;
    async fn recv(&mut self) -> Result<TransportEvent<MSG>, ()>;
}

pub trait RpcAnswer<Res> {
    fn ok(&self, res: Res);
    fn error(&self, code: u32, message: &str);
}

#[async_trait::async_trait]
pub trait TransportRpc<Req, Res> {
    async fn recv(&mut self) -> Result<(u8, Req, Box<dyn RpcAnswer<Res>>), ()>;
}

pub trait TransportConnector: Send + Sync {
    fn connect_to(
        &self,
        node_id: NodeId,
        dest: NodeAddr,
    ) -> Result<TransportPendingOutgoing, OutgoingConnectionError>;
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum ConnectionMsg<MSG> {
    Reliable { stream_id: u16, data: MSG },
    Unreliable { stream_id: u16, data: MSG },
}

#[derive(PartialEq, Debug, Clone)]
pub struct ConnectionStats {
    pub rtt_ms: u16,
    pub sending_kbps: u32,
    pub send_est_kbps: u32,
    pub loss_percent: u32,
    pub over_use: bool,
}

#[derive(PartialEq, Debug)]
pub enum ConnectionEvent<MSG> {
    Msg {
        service_id: u8,
        msg: ConnectionMsg<MSG>,
    },
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

pub trait ConnectionAcceptor: Send + Sync {
    fn accept(&self);
    fn reject(&self, err: ConnectionRejectReason);
}

pub trait ConnectionSender<MSG>: Send + Sync {
    fn remote_node_id(&self) -> NodeId;
    fn connection_id(&self) -> u32;
    fn remote_addr(&self) -> NodeAddr;
    fn send(&self, service_id: u8, msg: ConnectionMsg<MSG>);
    fn close(&self);
}

#[async_trait::async_trait]
pub trait ConnectionReceiver<MSG> {
    fn remote_node_id(&self) -> NodeId;
    fn connection_id(&self) -> u32;
    fn remote_addr(&self) -> NodeAddr;
    async fn poll(&mut self) -> Result<ConnectionEvent<MSG>, ()>;
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
        self.sender.send_blocking(Ok(()));
    }

    fn reject(&self, err: ConnectionRejectReason) {
        self.sender.send_blocking(Err(err));
    }
}

use async_std::channel::{bounded, Receiver, Sender};
use bluesea_identity::{PeerAddr, PeerId};
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;

pub struct TransportPendingOutgoing {
    pub connection_id: u32,
}

pub enum TransportEvent<MSG> {
    IncomingRequest(PeerId, u32, Box<dyn ConnectionAcceptor>),
    OutgoingRequest(PeerId, u32, Box<dyn ConnectionAcceptor>),
    Incoming(
        Arc<dyn ConnectionSender<MSG>>,
        Box<dyn ConnectionReceiver<MSG> + Send>,
    ),
    Outgoing(
        Arc<dyn ConnectionSender<MSG>>,
        Box<dyn ConnectionReceiver<MSG> + Send>,
    ),
    OutgoingError {
        peer_id: PeerId,
        connection_id: u32,
        err: OutgoingConnectionError,
    },
}

#[async_trait::async_trait]
pub trait Transport<MSG> {
    fn connector(&self) -> Arc<dyn TransportConnector>;
    async fn recv(&mut self) -> Result<TransportEvent<MSG>, ()>;
}

pub trait TransportConnector: Send + Sync {
    fn connect_to(
        &self,
        peer_id: PeerId,
        dest: PeerAddr,
    ) -> Result<TransportPendingOutgoing, OutgoingConnectionError>;
}

#[derive(PartialEq, Debug)]
pub enum ConnectionMsg<MSG> {
    Reliable { stream_id: u16, data: MSG },
    Unreliable { stream_id: u16, data: MSG },
}

#[derive(PartialEq, Debug, Clone)]
pub struct ConnectionStats {
    rtt_ms: u16,
    sending_kbps: u32,
    send_est_kbps: u32,
    loss_percent: u32,
    over_use: bool,
}

#[derive(PartialEq, Debug)]
pub enum ConnectionEvent<MSG> {
    Msg {
        service_id: u8,
        msg: ConnectionMsg<MSG>,
    },
    Stats(ConnectionStats),
}

#[derive(PartialEq, Error, Debug)]
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
    fn remote_peer_id(&self) -> PeerId;
    fn connection_id(&self) -> u32;
    fn remote_addr(&self) -> PeerAddr;
    fn send(&self, service_id: u8, msg: ConnectionMsg<MSG>);
    fn close(&self);
}

#[async_trait::async_trait]
pub trait ConnectionReceiver<MSG> {
    fn remote_peer_id(&self) -> PeerId;
    fn connection_id(&self) -> u32;
    fn remote_addr(&self) -> PeerAddr;
    async fn poll(&mut self) -> Result<ConnectionEvent<MSG>, ()>;
}

#[derive(PartialEq, Error, Debug)]
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

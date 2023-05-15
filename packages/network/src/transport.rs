use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use bluesea_identity::{PeerAddr, PeerId};

pub struct TransportPendingOutgoing {
    pub connection_id: u32,
}

pub enum TransportEvent {
    Incoming(
        Arc<dyn ConnectionSender>,
        Box<dyn ConnectionReceiver + Send>,
    ),
    Outgoing(
        Arc<dyn ConnectionSender>,
        Box<dyn ConnectionReceiver + Send>,
    ),
    OutgoingError {
        peer_id: PeerId,
        connection_id: u32,
        err: OutgoingConnectionError,
    },
}

#[async_trait::async_trait]
pub trait Transport {
    fn connector(&self) -> Box<dyn TransportConnector>;
    async fn recv(&mut self) -> Result<TransportEvent, ()>;
}

pub trait TransportConnector: Send + Sync {
    fn connect_to(
        &self,
        peer_id: PeerId,
        dest: PeerAddr,
    ) -> Result<TransportPendingOutgoing, OutgoingConnectionError>;
}

pub enum ConnectionEvent {
    Reliable {
        stream_id: u16,
        data: Vec<u8>,
    },
    Unreliable {
        stream_id: u16,
        data: Vec<u8>,
    },
    Stats {
        rtt_ms: (u16, u16),
        sending_kbps: u32,
        send_est_kbps: u32,
        loss_percent: u32,
        over_use: bool,
    }
}

pub trait ConnectionSender: Send + Sync {
    fn peer_id(&self) -> PeerId;
    fn connection_id(&self) -> u32;
    fn remote_addr(&self) -> PeerAddr;
    fn send_stream_reliable(&self, stream_id: u16, data: &[u8]);
    fn send_stream_unreliable(&self, stream_id: u16, data: &[u8]);
    fn close(&self);
}

#[async_trait::async_trait]
pub trait ConnectionReceiver {
    fn connection_id(&self) -> u32;
    fn remote_addr(&self) -> PeerAddr;
    async fn poll(&mut self) -> Result<ConnectionEvent, ()>;
}

#[derive(Error, Debug)]
pub enum OutgoingConnectionError {
    #[error("Too many connection")]
    TooManyConnection,
    #[error("Authentication Error")]
    AuthenticationError,
}

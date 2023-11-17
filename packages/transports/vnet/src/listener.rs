use crate::connection::VnetConnection;
use async_std::channel::Receiver;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::transport::{ConnectionAcceptor, OutgoingConnectionError, TransportOutgoingLocalUuid};

pub enum VnetListenerEvent {
    IncomingRequest(NodeId, ConnId, Box<dyn ConnectionAcceptor>),
    OutgoingRequest(NodeId, ConnId, TransportOutgoingLocalUuid, Box<dyn ConnectionAcceptor>),
    Incoming(VnetConnection),
    Outgoing(VnetConnection, TransportOutgoingLocalUuid),
    OutgoingErr(NodeId, ConnId, TransportOutgoingLocalUuid, OutgoingConnectionError),
}

pub struct VnetListener {
    pub(crate) rx: Receiver<VnetListenerEvent>,
}

impl VnetListener {
    pub async fn recv(&mut self) -> Option<VnetListenerEvent> {
        self.rx.recv().await.ok()
    }
}

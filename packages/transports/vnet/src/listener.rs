use crate::connection::VnetConnection;
use async_std::channel::Receiver;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::transport::{ConnectionAcceptor, OutgoingConnectionError};

pub enum VnetListenerEvent {
    IncomingRequest(NodeId, ConnId, Box<dyn ConnectionAcceptor>),
    Incoming(VnetConnection),
    Outgoing(VnetConnection),
    OutgoingErr(NodeId, ConnId, OutgoingConnectionError),
}

pub struct VnetListener {
    pub(crate) rx: Receiver<VnetListenerEvent>,
}

impl VnetListener {
    pub async fn recv(&mut self) -> Option<VnetListenerEvent> {
        self.rx.recv().await.ok()
    }
}

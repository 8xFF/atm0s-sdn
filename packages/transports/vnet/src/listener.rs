use crate::connection::VnetConnection;
use async_std::channel::Receiver;
use bluesea_identity::{ConnId, NodeId};
use network::transport::{ConnectionAcceptor, OutgoingConnectionError};

pub enum VnetListenerEvent<MSG> {
    IncomingRequest(NodeId, ConnId, Box<dyn ConnectionAcceptor>),
    OutgoingRequest(NodeId, ConnId, Box<dyn ConnectionAcceptor>),
    Incoming(VnetConnection<MSG>),
    Outgoing(VnetConnection<MSG>),
    OutgoingErr(NodeId, ConnId, OutgoingConnectionError),
}

pub struct VnetListener<MSG> {
    pub(crate) rx: Receiver<VnetListenerEvent<MSG>>,
}

impl<MSG> VnetListener<MSG> {
    pub async fn recv(&mut self) -> Option<VnetListenerEvent<MSG>> {
        self.rx.recv().await.ok()
    }
}

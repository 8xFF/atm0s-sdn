use crate::connection::VnetConnection;
use async_std::channel::Receiver;
use bluesea_identity::NodeId;
use network::transport::{ConnectionAcceptor, OutgoingConnectionError};

pub enum VnetListenerEvent<MSG> {
    IncomingRequest(NodeId, u32, Box<dyn ConnectionAcceptor>),
    OutgoingRequest(NodeId, u32, Box<dyn ConnectionAcceptor>),
    Incoming(VnetConnection<MSG>),
    Outgoing(VnetConnection<MSG>),
    OutgoingErr(u32, NodeId, OutgoingConnectionError),
}

pub struct VnetListener<MSG> {
    pub(crate) rx: Receiver<VnetListenerEvent<MSG>>,
}

impl<MSG> VnetListener<MSG> {
    pub async fn recv(&mut self) -> Option<VnetListenerEvent<MSG>> {
        self.rx.recv().await.ok()
    }
}

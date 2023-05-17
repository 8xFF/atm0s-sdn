use kanal::AsyncReceiver;
use bluesea_identity::PeerId;
use network::transport::OutgoingConnectionError;
use crate::connection::VnetConnection;

pub enum VnetListenerEvent<MSG> {
    Incoming(VnetConnection<MSG>),
    Outgoing(VnetConnection<MSG>),
    OutgoingErr(u32, PeerId, OutgoingConnectionError)
}

pub struct VnetListener<MSG> {
    pub(crate) rx: AsyncReceiver<VnetListenerEvent<MSG>>,
}

impl<MSG> VnetListener<MSG> {
    pub async fn recv(&mut self) -> Option<VnetListenerEvent<MSG>> {
        self.rx.recv().await.ok()
    }
}
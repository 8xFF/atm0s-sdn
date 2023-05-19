use async_std::channel::{Receiver, Sender};
use bluesea_identity::{PeerAddr, PeerId};
use network::transport::{ConnectionEvent, ConnectionMsg, ConnectionReceiver, ConnectionSender};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub type VnetConnection<MSG> = (
    Arc<VnetConnectionSender<MSG>>,
    Box<VnetConnectionReceiver<MSG>>,
);

pub struct VnetConnectionReceiver<MSG> {
    pub(crate) remote_peer_id: PeerId,
    pub(crate) conn_id: u32,
    pub(crate) remote_addr: PeerAddr,
    pub(crate) recv: Receiver<Option<(u8, ConnectionMsg<MSG>)>>,
    pub(crate) connections: Arc<RwLock<HashMap<u32, (PeerId, PeerId)>>>,
}

#[async_trait::async_trait]
impl<MSG> ConnectionReceiver<MSG> for VnetConnectionReceiver<MSG>
where
    MSG: Send + Sync,
{
    fn remote_peer_id(&self) -> bluesea_identity::PeerId {
        self.remote_peer_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> bluesea_identity::PeerAddr {
        self.remote_addr.clone()
    }

    async fn poll(&mut self) -> Result<ConnectionEvent<MSG>, ()> {
        if let Some((service_id, msg)) = self.recv.recv().await.map_err(|e| ())? {
            Ok(ConnectionEvent::Msg { msg, service_id })
        } else {
            //disconnected
            self.connections.write().remove(&self.conn_id);
            Err(())
        }
    }
}

pub struct VnetConnectionSender<MSG> {
    pub(crate) remote_peer_id: PeerId,
    pub(crate) conn_id: u32,
    pub(crate) remote_addr: PeerAddr,
    pub(crate) sender: Sender<Option<(u8, ConnectionMsg<MSG>)>>,
    pub(crate) remote_sender: Sender<Option<(u8, ConnectionMsg<MSG>)>>,
}

#[async_trait::async_trait]
impl<MSG> ConnectionSender<MSG> for VnetConnectionSender<MSG>
where
    MSG: Send + Sync,
{
    fn remote_peer_id(&self) -> bluesea_identity::PeerId {
        self.remote_peer_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> bluesea_identity::PeerAddr {
        self.remote_addr.clone()
    }

    fn send(&self, service_id: u8, msg: ConnectionMsg<MSG>) {
        self.remote_sender
            .send_blocking(Some((service_id, msg)))
            .unwrap();
    }

    fn close(&self) {
        self.sender.send_blocking(None).unwrap();
        self.remote_sender.send_blocking(None).unwrap();
    }
}

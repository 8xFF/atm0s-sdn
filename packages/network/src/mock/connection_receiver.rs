use crate::transport::{ConnectionEvent, ConnectionReceiver};
use async_std::channel::Receiver;
use bluesea_identity::{PeerAddr, PeerId};

pub struct MockConnectionReceiver<MSG> {
    pub remote_peer_id: PeerId,
    pub conn_id: u32,
    pub remote_addr: PeerAddr,
    pub receiver: Receiver<Option<ConnectionEvent<MSG>>>,
}

#[async_trait::async_trait]
impl<MSG: Send + Sync> ConnectionReceiver<MSG> for MockConnectionReceiver<MSG> {
    fn remote_peer_id(&self) -> PeerId {
        self.remote_peer_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> PeerAddr {
        self.remote_addr.clone()
    }

    async fn poll(&mut self) -> Result<ConnectionEvent<MSG>, ()> {
        let data = self.receiver.recv().await.map_err(|e| ())?;
        if let Some(data) = data {
            Ok(data)
        } else {
            Err(())
        }
    }
}

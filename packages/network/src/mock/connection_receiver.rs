use crate::transport::{ConnectionEvent, ConnectionReceiver};
use async_std::channel::Receiver;
use bluesea_identity::{NodeAddr, NodeId};

pub struct MockConnectionReceiver<MSG> {
    pub remote_node_id: NodeId,
    pub conn_id: u32,
    pub remote_addr: NodeAddr,
    pub receiver: Receiver<Option<ConnectionEvent<MSG>>>,
}

#[async_trait::async_trait]
impl<MSG: Send + Sync> ConnectionReceiver<MSG> for MockConnectionReceiver<MSG> {
    fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> NodeAddr {
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

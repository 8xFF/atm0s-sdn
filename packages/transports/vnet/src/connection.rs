use async_std::channel::{Receiver, Sender};
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use network::transport::{ConnectionEvent, ConnectionMsg, ConnectionReceiver, ConnectionSender};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub type VnetConnection<MSG> = (
    Arc<VnetConnectionSender<MSG>>,
    Box<VnetConnectionReceiver<MSG>>,
);

pub struct VnetConnectionReceiver<MSG> {
    pub(crate) remote_node_id: NodeId,
    pub(crate) conn_id: ConnId,
    pub(crate) remote_addr: NodeAddr,
    pub(crate) recv: Receiver<Option<(u8, ConnectionMsg<MSG>)>>,
    pub(crate) connections: Arc<RwLock<HashMap<ConnId, (NodeId, NodeId)>>>,
}

#[async_trait::async_trait]
impl<MSG> ConnectionReceiver<MSG> for VnetConnectionReceiver<MSG>
where
    MSG: Send + Sync,
{
    fn remote_node_id(&self) -> bluesea_identity::NodeId {
        self.remote_node_id
    }

    fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    fn remote_addr(&self) -> bluesea_identity::NodeAddr {
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
    pub(crate) remote_node_id: NodeId,
    pub(crate) conn_id: ConnId,
    pub(crate) remote_addr: NodeAddr,
    pub(crate) sender: Sender<Option<(u8, ConnectionMsg<MSG>)>>,
    pub(crate) remote_sender: Sender<Option<(u8, ConnectionMsg<MSG>)>>,
}

#[async_trait::async_trait]
impl<MSG> ConnectionSender<MSG> for VnetConnectionSender<MSG>
where
    MSG: Send + Sync,
{
    fn remote_node_id(&self) -> bluesea_identity::NodeId {
        self.remote_node_id
    }

    fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    fn remote_addr(&self) -> bluesea_identity::NodeAddr {
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

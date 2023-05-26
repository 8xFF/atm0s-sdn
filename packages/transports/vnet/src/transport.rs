use crate::connection::VnetConnection;
use crate::connector::VnetConnector;
use crate::earth::VnetEarth;
use crate::listener::{VnetListener, VnetListenerEvent};
use bluesea_identity::{NodeAddr, NodeId};
use network::transport::{Transport, TransportConnector, TransportEvent};
use std::sync::Arc;

pub struct VnetTransport<MSG> {
    port: u64,
    earth: Arc<VnetEarth<MSG>>,
    listener: VnetListener<MSG>,
    connector: Arc<VnetConnector<MSG>>,
}

impl<MSG> VnetTransport<MSG>
where
    MSG: Send + Sync + 'static,
{
    pub fn new(earth: Arc<VnetEarth<MSG>>, port: u64, node: NodeId, addr: NodeAddr) -> Self {
        Self {
            listener: earth.create_listener(port, node, addr),
            connector: Arc::new(VnetConnector {
                port,
                earth: earth.clone(),
            }),
            earth,
            port,
        }
    }
}

#[async_trait::async_trait]
impl<MSG> Transport<MSG> for VnetTransport<MSG>
where
    MSG: Send + Sync + 'static,
{
    fn connector(&self) -> Arc<dyn TransportConnector> {
        self.connector.clone()
    }

    async fn recv(&mut self) -> Result<TransportEvent<MSG>, ()> {
        match self.listener.recv().await {
            None => Err(()),
            Some(VnetListenerEvent::IncomingRequest(node, conn, acceptor)) => {
                Ok(TransportEvent::IncomingRequest(node, conn, acceptor))
            }
            Some(VnetListenerEvent::OutgoingRequest(node, conn, acceptor)) => {
                Ok(TransportEvent::OutgoingRequest(node, conn, acceptor))
            }
            Some(VnetListenerEvent::Incoming((sender, recv))) => {
                Ok(TransportEvent::Incoming(sender, recv))
            }
            Some(VnetListenerEvent::Outgoing((sender, recv))) => {
                Ok(TransportEvent::Outgoing(sender, recv))
            }
            Some(VnetListenerEvent::OutgoingErr(connection_id, node_id, err)) => {
                Ok(TransportEvent::OutgoingError {
                    connection_id,
                    node_id: node_id,
                    err,
                })
            }
        }
    }
}

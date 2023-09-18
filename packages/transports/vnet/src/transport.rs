use crate::connector::VnetConnector;
use crate::earth::VnetEarth;
use crate::listener::{VnetListener, VnetListenerEvent};
use bluesea_identity::{NodeAddr, NodeId};
use network::transport::{Transport, TransportConnector, TransportEvent};
use std::sync::Arc;

pub struct VnetTransport {
    #[allow(unused)]
    port: u64,
    #[allow(unused)]
    earth: Arc<VnetEarth>,
    listener: VnetListener,
    connector: Arc<VnetConnector>,
}

impl VnetTransport {
    pub fn new(earth: Arc<VnetEarth>, port: u64, node: NodeId, addr: NodeAddr) -> Self {
        Self {
            listener: earth.create_listener(port, node, addr),
            connector: Arc::new(VnetConnector { port, earth: earth.clone() }),
            earth,
            port,
        }
    }
}

#[async_trait::async_trait]
impl Transport for VnetTransport {
    fn connector(&self) -> Arc<dyn TransportConnector> {
        self.connector.clone()
    }

    async fn recv(&mut self) -> Result<TransportEvent, ()> {
        match self.listener.recv().await {
            None => Err(()),
            Some(VnetListenerEvent::IncomingRequest(node, conn, acceptor)) => Ok(TransportEvent::IncomingRequest(node, conn, acceptor)),
            Some(VnetListenerEvent::OutgoingRequest(node, conn, acceptor)) => Ok(TransportEvent::OutgoingRequest(node, conn, acceptor)),
            Some(VnetListenerEvent::Incoming((sender, recv))) => Ok(TransportEvent::Incoming(sender, recv)),
            Some(VnetListenerEvent::Outgoing((sender, recv))) => Ok(TransportEvent::Outgoing(sender, recv)),
            Some(VnetListenerEvent::OutgoingErr(node_id, conn_id, err)) => Ok(TransportEvent::OutgoingError { node_id, conn_id, err }),
        }
    }
}

use crate::connector::VnetConnector;
use crate::earth::VnetEarth;
use crate::listener::{VnetListener, VnetListenerEvent};
use p_8xff_sdn_identity::{NodeAddr, NodeId};
use p_8xff_sdn_network::transport::{Transport, TransportConnector, TransportEvent};
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
            Some(VnetListenerEvent::OutgoingRequest(node, conn, local_uuid, acceptor)) => Ok(TransportEvent::OutgoingRequest(node, conn, acceptor, local_uuid)),
            Some(VnetListenerEvent::Incoming((sender, recv))) => Ok(TransportEvent::Incoming(sender, recv)),
            Some(VnetListenerEvent::Outgoing((sender, recv), local_uuid)) => Ok(TransportEvent::Outgoing(sender, recv, local_uuid)),
            Some(VnetListenerEvent::OutgoingErr(node_id, conn_id, local_uuid, err)) => Ok(TransportEvent::OutgoingError {
                node_id,
                conn_id: Some(conn_id),
                err,
                local_uuid,
            }),
        }
    }
}

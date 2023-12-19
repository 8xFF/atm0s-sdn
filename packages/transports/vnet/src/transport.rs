use crate::connector::VnetConnector;
use crate::earth::VnetEarth;
use crate::listener::{VnetListener, VnetListenerEvent};
use atm0s_sdn_identity::NodeAddr;
use atm0s_sdn_network::transport::{Transport, TransportConnector, TransportEvent};
use std::sync::Arc;

pub struct VnetTransport {
    #[allow(unused)]
    port: u32,
    #[allow(unused)]
    earth: Arc<VnetEarth>,
    listener: VnetListener,
    connector: VnetConnector,
}

impl VnetTransport {
    pub fn new(earth: Arc<VnetEarth>, addr: NodeAddr) -> Self {
        Self {
            port: addr.node_id(),
            connector: VnetConnector::new(addr.node_id(), earth.clone()),
            listener: earth.create_listener(addr),
            earth,
        }
    }
}

#[async_trait::async_trait]
impl Transport for VnetTransport {
    fn connector(&mut self) -> &mut dyn TransportConnector {
        &mut self.connector
    }

    async fn recv(&mut self) -> Result<TransportEvent, ()> {
        match self.listener.recv().await {
            None => Err(()),
            Some(VnetListenerEvent::IncomingRequest(node, conn, acceptor)) => Ok(TransportEvent::IncomingRequest(node, conn, acceptor)),
            Some(VnetListenerEvent::Incoming((sender, recv))) => Ok(TransportEvent::Incoming(sender, recv)),
            Some(VnetListenerEvent::Outgoing((sender, recv))) => Ok(TransportEvent::Outgoing(sender, recv)),
            Some(VnetListenerEvent::OutgoingErr(node_id, conn_id, err)) => Ok(TransportEvent::OutgoingError { node_id, conn_id, err }),
        }
    }
}

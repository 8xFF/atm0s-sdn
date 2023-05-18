use crate::connection::VnetConnection;
use crate::connector::VnetConnector;
use crate::earth::VnetEarth;
use crate::listener::{VnetListener, VnetListenerEvent};
use bluesea_identity::{PeerAddr, PeerId};
use network::transport::{Transport, TransportConnector, TransportEvent};
use std::sync::Arc;

pub struct VnetTransport<MSG> {
    port: u64,
    earth: Arc<VnetEarth<MSG>>,
    listener: VnetListener<MSG>,
    connector: Arc<VnetConnector<MSG>>,
}

impl<MSG> VnetTransport<MSG> {
    pub fn new(earth: Arc<VnetEarth<MSG>>, port: u64, peer: PeerId, addr: PeerAddr) -> Self {
        Self {
            listener: earth.create_listener(port, peer, addr),
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
            Some(VnetListenerEvent::Incoming((sender, recv))) => {
                Ok(TransportEvent::Incoming(sender, recv))
            }
            Some(VnetListenerEvent::Outgoing((sender, recv))) => {
                Ok(TransportEvent::Outgoing(sender, recv))
            }
            Some(VnetListenerEvent::OutgoingErr(connection_id, peer_id, err)) => {
                Ok(TransportEvent::OutgoingError {
                    connection_id,
                    peer_id,
                    err,
                })
            }
        }
    }
}

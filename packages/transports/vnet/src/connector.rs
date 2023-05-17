use std::sync::Arc;
use bluesea_identity::multiaddr::Protocol;
use network::transport::{OutgoingConnectionError, TransportConnector, TransportPendingOutgoing};
use crate::earth::VnetEarth;

pub struct VnetConnector<MSG> {
    pub(crate) port: u64,
    pub(crate) earth: Arc<VnetEarth<MSG>>
}

impl<MSG> TransportConnector for VnetConnector<MSG>
    where MSG: Send + Sync
{
    fn connect_to(&self, peer_id: bluesea_identity::PeerId, addr: bluesea_identity::PeerAddr) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        for protocol in &addr {
            match protocol {
                Protocol::Memory(port) => {
                    if let Some(connection_id) = self.earth.create_outgoing(self.port, peer_id, port) {
                        return Ok(TransportPendingOutgoing {
                            connection_id
                        });
                    } else {
                        return Err(OutgoingConnectionError::DestinationNotFound);
                    }
                },
                _ => {

                }
            }
        }
        Err(OutgoingConnectionError::UnsupportedProtocol)
    }
}
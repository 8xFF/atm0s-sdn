use crate::earth::VnetEarth;
use bluesea_identity::Protocol;
use network::transport::{OutgoingConnectionError, TransportConnectingOutgoing, TransportConnector};
use std::sync::Arc;

pub struct VnetConnector {
    pub(crate) port: u64,
    pub(crate) earth: Arc<VnetEarth>,
}

impl TransportConnector for VnetConnector {
    fn connect_to(&self, node_id: bluesea_identity::NodeId, addr: bluesea_identity::NodeAddr) -> Result<TransportConnectingOutgoing, OutgoingConnectionError> {
        for protocol in &addr {
            if let Protocol::Memory(port) = protocol {
                if let Some(conn_id) = self.earth.create_outgoing(self.port, node_id, port) {
                    return Ok(TransportConnectingOutgoing { conn_id });
                } else {
                    return Err(OutgoingConnectionError::DestinationNotFound);
                }
            }
        }
        Err(OutgoingConnectionError::UnsupportedProtocol)
    }
}

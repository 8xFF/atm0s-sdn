use crate::earth::VnetEarth;
use p_8xff_sdn_identity::Protocol;
use p_8xff_sdn_network::transport::{OutgoingConnectionError, TransportConnector, TransportOutgoingLocalUuid};
use std::sync::Arc;

pub struct VnetConnector {
    pub(crate) port: u64,
    pub(crate) earth: Arc<VnetEarth>,
}

impl TransportConnector for VnetConnector {
    fn connect_to(&self, local_uuid: TransportOutgoingLocalUuid, node_id: p_8xff_sdn_identity::NodeId, addr: p_8xff_sdn_identity::NodeAddr) -> Result<(), OutgoingConnectionError> {
        for protocol in &addr {
            if let Protocol::Memory(port) = protocol {
                if let Some(_conn_id) = self.earth.create_outgoing(local_uuid, self.port, node_id, port) {
                    return Ok(());
                } else {
                    return Err(OutgoingConnectionError::DestinationNotFound);
                }
            }
        }
        Err(OutgoingConnectionError::UnsupportedProtocol)
    }
}

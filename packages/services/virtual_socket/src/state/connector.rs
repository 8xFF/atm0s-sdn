use std::{collections::HashMap, sync::Arc};

use atm0s_sdn_identity::NodeId;
use parking_lot::RwLock;

use super::{socket::VirtualSocket, State, VirtualSocketConnectResult};

#[derive(Debug)]
pub enum VirtualSocketConnectorError {
    Timeout,
    Unreachable,
}

pub struct VirtualSocketConnector {
    pub(crate) state: Arc<RwLock<State>>,
}

impl VirtualSocketConnector {
    pub async fn connect_to(&self, secure: bool, dest: NodeId, listener: &str, meta: HashMap<String, String>) -> Result<VirtualSocket, VirtualSocketConnectorError> {
        let rx = self.state.write().new_outgoing(secure, dest, listener, meta).ok_or(VirtualSocketConnectorError::Unreachable)?;
        match rx.recv().await {
            Ok(VirtualSocketConnectResult::Success(builder)) => Ok(builder.build(self.state.clone())),
            Ok(VirtualSocketConnectResult::Timeout) => Err(VirtualSocketConnectorError::Timeout),
            Ok(VirtualSocketConnectResult::Unreachable) => Err(VirtualSocketConnectorError::Unreachable),
            Err(_) => Err(VirtualSocketConnectorError::Unreachable),
        }
    }
}

use std::{collections::HashMap, sync::Arc};

use p_8xff_sdn_identity::ConnId;
use p_8xff_sdn_network::{msg::TransportMsg, transport::ConnectionSender};

pub struct RemoteRelay {
    remotes: HashMap<ConnId, Arc<dyn ConnectionSender>>,
}

impl RemoteRelay {
    pub fn new() -> Self {
        Self { remotes: HashMap::new() }
    }

    pub fn on_connection_opened(&mut self, conn_id: ConnId, sender: Arc<dyn ConnectionSender>) {
        self.remotes.insert(conn_id, sender);
    }

    pub fn on_connection_closed(&mut self, conn_id: ConnId) {
        self.remotes.remove(&conn_id);
    }

    pub fn relay(&self, remotes: &[ConnId], msg: &TransportMsg) {
        for remote in remotes {
            if let Some(sender) = self.remotes.get(remote) {
                log::trace!("[RemoteRelay] relay to remote {}", remote);
                sender.send(msg.clone());
            }
        }
    }
}

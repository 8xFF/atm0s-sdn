use std::collections::HashMap;

use bluesea_identity::ConnId;
use network::{msg::TransportMsg, ConnectionAgent};

use crate::msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};

pub struct RemoteRelay<BE, HE> {
    remotes: HashMap<ConnId, ConnectionAgent<BE, HE>>,
}

impl<BE, HE> RemoteRelay<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self { remotes: HashMap::new() }
    }

    pub fn on_connection_opened(&mut self, agent: &ConnectionAgent<BE, HE>) {
        self.remotes.insert(agent.conn_id(), agent.clone());
    }

    pub fn on_connection_closed(&mut self, agent: &ConnectionAgent<BE, HE>) {
        self.remotes.remove(&agent.conn_id());
    }

    pub fn relay(&self, remotes: &[ConnId], msg: &TransportMsg) {
        for remote in remotes {
            if let Some(agent) = self.remotes.get(remote) {
                agent.send_net(msg.clone());
            }
        }
    }
}

use std::{fmt::Display, sync::Arc};

use bluesea_identity::{ConnId, NodeId};
use bytes::Bytes;
use network::msg::TransportMsg;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use utils::{awaker::Awaker, SystemTimer};

use crate::{
    msg::{PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    PubsubSdk,
};

use self::{
    feedback::FeedbackConsumerId,
    local::LocalRelay,
    logic::{PubsubRelayLogic, PubsubRelayLogicOutput},
    remote::RemoteRelay,
};

pub(crate) mod feedback;
pub(crate) mod local;
pub(crate) mod logic;
pub(crate) mod remote;

pub type ChannelUuid = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelIdentify(ChannelUuid, NodeId);
pub type LocalPubId = u64;
pub type LocalSubId = u64;

impl Display for ChannelIdentify {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.0, self.1)
    }
}

impl ChannelIdentify {
    pub fn new(uuid: ChannelUuid, source: NodeId) -> Self {
        Self(uuid, source)
    }

    pub fn uuid(&self) -> ChannelUuid {
        self.0
    }

    pub fn source(&self) -> NodeId {
        self.1
    }
}

pub struct PubsubRelay<BE, HE> {
    logic: Arc<RwLock<PubsubRelayLogic>>,
    remote: Arc<RwLock<RemoteRelay<BE, HE>>>,
    local: Arc<RwLock<LocalRelay>>,
}

impl<BE, HE> Clone for PubsubRelay<BE, HE> {
    fn clone(&self) -> Self {
        Self {
            logic: self.logic.clone(),
            remote: self.remote.clone(),
            local: self.local.clone(),
        }
    }
}

impl<BE, HE> PubsubRelay<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(node_id: NodeId, awaker: Arc<dyn Awaker>) -> (Self, PubsubSdk<BE, HE>) {
        let s = Self {
            logic: Arc::new(RwLock::new(PubsubRelayLogic::new(node_id, Arc::new(SystemTimer()), awaker))),
            remote: Arc::new(RwLock::new(RemoteRelay::new())),
            local: Arc::new(RwLock::new(LocalRelay::new())),
        };
        let sdk = PubsubSdk::new(node_id, s.logic.clone(), s.remote.clone(), s.local.clone());
        (s, sdk)
    }

    pub fn on_connection_opened(&self, agent: &network::ConnectionAgent<BE, HE>) {
        self.remote.write().on_connection_opened(agent);
    }

    pub fn on_connection_closed(&self, agent: &network::ConnectionAgent<BE, HE>) {
        self.remote.write().on_connection_closed(agent);
    }

    pub fn tick(&self) {
        let local_fbs = self.logic.write().tick();
        for fb in local_fbs {
            self.local.read().feedback(fb.channel.uuid(), fb);
        }
    }

    pub fn on_event(&self, from: NodeId, conn: ConnId, event: PubsubRemoteEvent) {
        self.logic.write().on_event(from, conn, event);
    }

    pub fn on_feedback(&self, channel: ChannelIdentify, _from: NodeId, conn: ConnId, fb: feedback::Feedback) {
        if let Some(local_fb) = self.logic.write().on_feedback(channel, FeedbackConsumerId::Remote(conn), fb) {
            self.local.read().feedback(channel.uuid(), local_fb);
        }
    }

    pub fn relay(&self, channel: ChannelIdentify, msg: TransportMsg) {
        if let Some((remotes, locals)) = self.logic.read().relay(channel) {
            self.remote.read().relay(remotes, &msg);
            if !locals.is_empty() {
                self.local.read().relay(locals, Bytes::from(msg.payload().to_vec()));
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<(NodeId, Option<ConnId>, PubsubRelayLogicOutput)> {
        self.logic.write().pop_action()
    }
}

use std::{fmt::Display, sync::Arc};

use bytes::Bytes;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{msg::TransportMsg, transport::ConnectionSender};
use atm0s_sdn_utils::{awaker::Awaker, Timer};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::{msg::PubsubRemoteEvent, PubsubSdk};

use self::{
    feedback::FeedbackConsumerId,
    local::{LocalRelay, LocalRelayAction},
    logic::{PubsubRelayLogic, PubsubRelayLogicOutput},
    remote::RemoteRelay,
    source_binding::{SourceBinding, SourceBindingAction},
};

pub(crate) mod feedback;
pub(crate) mod local;
pub(crate) mod logic;
pub(crate) mod remote;
pub(crate) mod source_binding;

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

pub struct PubsubRelay {
    logic: Arc<RwLock<PubsubRelayLogic>>,
    remote: Arc<RwLock<RemoteRelay>>,
    local: Arc<RwLock<LocalRelay>>,
    source_binding: Arc<RwLock<SourceBinding>>,
}

impl Clone for PubsubRelay {
    fn clone(&self) -> Self {
        Self {
            logic: self.logic.clone(),
            remote: self.remote.clone(),
            local: self.local.clone(),
            source_binding: self.source_binding.clone(),
        }
    }
}

impl PubsubRelay {
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>) -> (Self, PubsubSdk) {
        let s = Self {
            logic: Arc::new(RwLock::new(PubsubRelayLogic::new(node_id))),
            remote: Arc::new(RwLock::new(RemoteRelay::new())),
            local: Arc::new(RwLock::new(LocalRelay::new())),
            source_binding: Arc::new(RwLock::new(SourceBinding::new())),
        };
        let sdk = PubsubSdk::new(node_id, s.logic.clone(), s.remote.clone(), s.local.clone(), s.source_binding.clone(), timer);
        (s, sdk)
    }

    pub fn set_awaker(&self, awaker: Arc<dyn Awaker>) {
        self.logic.write().set_awaker(awaker.clone());
        self.local.write().set_awaker(awaker.clone());
        self.source_binding.write().set_awaker(awaker);
    }

    pub fn on_connection_opened(&self, conn_id: ConnId, sender: Arc<dyn ConnectionSender>) {
        self.remote.write().on_connection_opened(conn_id, sender);
    }

    pub fn on_connection_closed(&self, conn_id: ConnId) {
        self.remote.write().on_connection_closed(conn_id);
    }

    pub fn tick(&self, now_ms: u64) {
        let local_fbs = self.logic.write().tick(now_ms);
        for fb in local_fbs {
            self.local.read().feedback(fb.channel.uuid(), fb);
        }
    }

    pub fn on_source_added(&self, channel: ChannelUuid, source: NodeId) {
        if let Some(subs) = self.source_binding.write().on_source_added(channel, source) {
            log::debug!("[PubsubRelay] channel {} added source  {} => auto sub for local subs {:?}", channel, source, subs);
            for sub in subs {
                self.logic.write().on_local_sub(ChannelIdentify::new(channel, source), sub);
            }
        }
    }

    pub fn on_source_removed(&self, channel: ChannelUuid, source: NodeId) {
        if let Some(subs) = self.source_binding.write().on_source_removed(channel, source) {
            log::debug!("[PubsubRelay] channel {} removed source {} => auto unsub for local subs {:?}", channel, source, subs);
            for sub in subs {
                self.logic.write().on_local_unsub(ChannelIdentify::new(channel, source), sub);
            }
        }
    }

    pub fn on_event(&self, now_ms: u64, from: NodeId, conn: ConnId, event: PubsubRemoteEvent) {
        self.logic.write().on_event(now_ms, from, conn, event);
    }

    pub fn on_feedback(&self, now_ms: u64, channel: ChannelIdentify, _from: NodeId, conn: ConnId, fb: feedback::Feedback) {
        if let Some(local_fb) = self.logic.write().on_feedback(now_ms, channel, FeedbackConsumerId::Remote(conn), fb) {
            self.local.read().feedback(channel.uuid(), local_fb);
        }
    }

    pub fn relay(&self, channel: ChannelIdentify, msg: TransportMsg) {
        if let Some((remotes, locals)) = self.logic.read().relay(channel) {
            self.remote.read().relay(remotes, &msg);
            if !locals.is_empty() {
                self.local.read().relay(channel.source(), channel.uuid(), locals, Bytes::from(msg.payload().to_vec()));
            } else {
                log::trace!("No local subscriber for channel {}", channel);
            }
        }
    }

    pub fn pop_logic_action(&mut self) -> Option<(NodeId, Option<ConnId>, PubsubRelayLogicOutput)> {
        self.logic.write().pop_action()
    }

    pub fn pop_local_action(&mut self) -> Option<LocalRelayAction> {
        self.local.write().pop_action()
    }

    pub fn pop_source_binding_action(&mut self) -> Option<SourceBindingAction> {
        self.source_binding.write().pop_action()
    }
}

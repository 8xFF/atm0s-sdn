use std::sync::Arc;

use bluesea_identity::NodeId;
use bytes::Bytes;
use parking_lot::RwLock;

use crate::relay::{
    feedback::{Feedback, FeedbackConsumerId, FeedbackType},
    local::LocalRelay,
    logic::PubsubRelayLogic,
    source_binding::SourceBinding,
    ChannelIdentify, ChannelUuid, LocalSubId,
};

pub struct ConsumerRaw {
    sub_uuid: LocalSubId,
    channel: ChannelUuid,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    local: Arc<RwLock<LocalRelay>>,
    source_binding: Arc<RwLock<SourceBinding>>,
}

impl ConsumerRaw {
    pub fn new(
        sub_uuid: LocalSubId,
        channel: ChannelUuid,
        logic: Arc<RwLock<PubsubRelayLogic>>,
        local: Arc<RwLock<LocalRelay>>,
        source_binding: Arc<RwLock<SourceBinding>>,
        tx: async_std::channel::Sender<(LocalSubId, NodeId, ChannelUuid, Bytes)>,
    ) -> Self {
        local.write().on_local_sub(sub_uuid, tx);
        if let Some(sources) = source_binding.write().on_local_sub(channel, sub_uuid) {
            for source in sources {
                let channel = ChannelIdentify::new(channel, source);
                logic.write().on_local_sub(channel, sub_uuid);
            }
        }

        Self {
            sub_uuid,
            channel,
            logic,
            local,
            source_binding,
        }
    }

    pub fn uuid(&self) -> LocalSubId {
        self.sub_uuid
    }

    pub fn feedback(&self, id: u8, feedback_type: FeedbackType) {
        let sources = self.source_binding.read().sources_for(self.channel);
        for source in sources {
            let channel = ChannelIdentify::new(self.channel, source);
            let fb = Feedback {
                channel,
                id,
                feedback_type: feedback_type.clone(),
            };
            if let Some(local_fb) = self.logic.write().on_feedback(channel, FeedbackConsumerId::Local(self.sub_uuid), fb) {
                self.local.read().feedback(self.channel, local_fb);
            }
        }
    }
}

impl Drop for ConsumerRaw {
    fn drop(&mut self) {
        self.local.write().on_local_unsub(self.sub_uuid);
        if let Some(sources) = self.source_binding.write().on_local_sub(self.channel, self.sub_uuid) {
            for source in sources {
                let channel = ChannelIdentify::new(self.channel, source);
                self.logic.write().on_local_unsub(channel, self.sub_uuid);
            }
        }
    }
}

use std::sync::Arc;

use bytes::Bytes;
use parking_lot::RwLock;

use crate::relay::{
    feedback::{Feedback, FeedbackConsumerId, FeedbackType},
    local::LocalRelay,
    logic::PubsubRelayLogic,
    ChannelIdentify, LocalSubId,
};

pub struct Consumer {
    uuid: LocalSubId,
    channel: ChannelIdentify,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    local: Arc<RwLock<LocalRelay>>,
    rx: async_std::channel::Receiver<Bytes>,
}

impl Consumer {
    pub fn new(uuid: LocalSubId, channel: ChannelIdentify, logic: Arc<RwLock<PubsubRelayLogic>>, local: Arc<RwLock<LocalRelay>>, max_queue_size: usize) -> Self {
        let (tx, rx) = async_std::channel::bounded(max_queue_size);
        logic.write().on_local_sub(channel, uuid);
        local.write().on_local_sub(uuid, tx);

        Self { uuid, channel, logic, local, rx }
    }

    pub fn feedback(&self, id: u8, feedback_type: FeedbackType) {
        let fb = Feedback {
            channel: self.channel,
            id,
            feedback_type,
        };
        if let Some(local_fb) = self.logic.write().on_feedback(self.channel, FeedbackConsumerId::Local(self.uuid), fb) {
            self.local.read().feedback(self.channel.uuid(), local_fb);
        }
    }

    pub async fn recv(&self) -> Option<Bytes> {
        self.rx.recv().await.ok()
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        self.logic.write().on_local_unsub(self.channel, self.uuid);
        self.local.write().on_local_unsub(self.uuid);
    }
}

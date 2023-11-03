use std::sync::Arc;

use bluesea_identity::NodeId;
use bytes::Bytes;
use parking_lot::RwLock;
use utils::Timer;

use crate::relay::{
    feedback::{Feedback, FeedbackConsumerId, FeedbackType},
    local::LocalRelay,
    logic::PubsubRelayLogic,
    source_binding::SourceBinding,
    ChannelIdentify, ChannelUuid, LocalSubId,
};

pub struct Consumer {
    uuid: LocalSubId,
    channel: ChannelUuid,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    local: Arc<RwLock<LocalRelay>>,
    source_binding: Arc<RwLock<SourceBinding>>,
    rx: async_std::channel::Receiver<(LocalSubId, NodeId, ChannelUuid, Bytes)>,
    timer: Arc<dyn Timer>,
}

impl Consumer {
    pub fn new(
        uuid: LocalSubId,
        channel: ChannelUuid,
        logic: Arc<RwLock<PubsubRelayLogic>>,
        local: Arc<RwLock<LocalRelay>>,
        source_binding: Arc<RwLock<SourceBinding>>,
        max_queue_size: usize,
        timer: Arc<dyn Timer>,
    ) -> Self {
        let (tx, rx) = async_std::channel::bounded(max_queue_size);
        local.write().on_local_sub(uuid, tx);
        if let Some(sources) = source_binding.write().on_local_sub(channel, uuid) {
            for source in sources {
                let channel = ChannelIdentify::new(channel, source);
                logic.write().on_local_sub(channel, uuid);
            }
        }

        Self {
            uuid,
            channel,
            logic,
            local,
            source_binding,
            rx,
            timer,
        }
    }

    pub fn uuid(&self) -> LocalSubId {
        self.uuid
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
            if let Some(local_fb) = self.logic.write().on_feedback(self.timer.now_ms(), channel, FeedbackConsumerId::Local(self.uuid), fb) {
                self.local.read().feedback(self.channel, local_fb);
            }
        }
    }

    pub async fn recv(&self) -> Option<(LocalSubId, NodeId, ChannelUuid, Bytes)> {
        self.rx.recv().await.ok()
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        self.local.write().on_local_unsub(self.uuid);
        if let Some(sources) = self.source_binding.write().on_local_unsub(self.channel, self.uuid) {
            for source in sources {
                let channel = ChannelIdentify::new(self.channel, source);
                self.logic.write().on_local_unsub(channel, self.uuid);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use parking_lot::RwLock;
    use utils::MockTimer;

    use crate::{
        relay::{local::LocalRelay, logic::PubsubRelayLogic, source_binding::SourceBinding},
        ChannelIdentify, Consumer,
    };
    #[test]
    fn correct_create_and_destroy() {
        let node_id = 1;
        let channel = 1111;
        let channel_source = 2;
        let source_binding = Arc::new(RwLock::new(SourceBinding::new()));

        source_binding.write().on_source_added(channel, channel_source);

        let logic = Arc::new(RwLock::new(PubsubRelayLogic::new(node_id)));
        let local = Arc::new(RwLock::new(LocalRelay::new()));
        let timer = Arc::new(MockTimer::default());
        let sub_uuid = 10000;
        let consumer = Consumer::new(sub_uuid, channel, logic, local, source_binding, 100, timer);

        assert_eq!(
            consumer.logic.read().relay(ChannelIdentify::new(channel, channel_source)),
            Some((vec![].as_slice(), vec![sub_uuid].as_slice()))
        );
        assert_eq!(consumer.source_binding.read().consumers_for(channel), vec![sub_uuid]);

        let sb = consumer.source_binding.clone();
        let logic = consumer.logic.clone();
        drop(consumer);

        assert_eq!(logic.read().relay(ChannelIdentify::new(channel, channel_source)), None);
        assert_eq!(sb.read().consumers_for(channel), vec![]);
    }
}

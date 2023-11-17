use std::sync::Arc;

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_utils::Timer;
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
    timer: Arc<dyn Timer>,
}

impl ConsumerRaw {
    pub fn new(
        sub_uuid: LocalSubId,
        channel: ChannelUuid,
        logic: Arc<RwLock<PubsubRelayLogic>>,
        local: Arc<RwLock<LocalRelay>>,
        source_binding: Arc<RwLock<SourceBinding>>,
        tx: async_std::channel::Sender<(LocalSubId, NodeId, ChannelUuid, Bytes)>,
        timer: Arc<dyn Timer>,
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
            timer,
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
            if let Some(local_fb) = self.logic.write().on_feedback(self.timer.now_ms(), channel, FeedbackConsumerId::Local(self.sub_uuid), fb) {
                self.local.read().feedback(self.channel, local_fb);
            }
        }
    }
}

impl Drop for ConsumerRaw {
    fn drop(&mut self) {
        self.local.write().on_local_unsub(self.sub_uuid);
        if let Some(sources) = self.source_binding.write().on_local_unsub(self.channel, self.sub_uuid) {
            for source in sources {
                let channel = ChannelIdentify::new(self.channel, source);
                self.logic.write().on_local_unsub(channel, self.sub_uuid);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use atm0s_sdn_utils::MockTimer;
    use parking_lot::RwLock;

    use crate::{
        relay::{local::LocalRelay, logic::PubsubRelayLogic, source_binding::SourceBinding},
        ChannelIdentify, ConsumerRaw,
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
        let (tx, _rx) = async_std::channel::bounded(100);
        let consumer = ConsumerRaw::new(sub_uuid, channel, logic, local, source_binding, tx, timer);

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

use std::sync::Arc;

use bytes::Bytes;
use p_8xff_sdn_identity::NodeId;
use p_8xff_sdn_utils::Timer;
use parking_lot::RwLock;

use crate::relay::{
    feedback::{Feedback, FeedbackConsumerId, FeedbackType},
    local::LocalRelay,
    logic::PubsubRelayLogic,
    ChannelIdentify, ChannelUuid, LocalSubId,
};

pub struct ConsumerSingle {
    uuid: LocalSubId,
    channel: ChannelIdentify,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    local: Arc<RwLock<LocalRelay>>,
    rx: async_std::channel::Receiver<(LocalSubId, NodeId, ChannelUuid, Bytes)>,
    timer: Arc<dyn Timer>,
}

impl ConsumerSingle {
    pub fn new(uuid: LocalSubId, channel: ChannelIdentify, logic: Arc<RwLock<PubsubRelayLogic>>, local: Arc<RwLock<LocalRelay>>, max_queue_size: usize, timer: Arc<dyn Timer>) -> Self {
        let (tx, rx) = async_std::channel::bounded(max_queue_size);
        logic.write().on_local_sub(channel, uuid);
        local.write().on_local_sub(uuid, tx);

        Self {
            uuid,
            channel,
            logic,
            local,
            rx,
            timer,
        }
    }

    pub fn uuid(&self) -> LocalSubId {
        self.uuid
    }

    pub fn feedback(&self, id: u8, feedback_type: FeedbackType) {
        let fb = Feedback {
            channel: self.channel,
            id,
            feedback_type,
        };
        if let Some(local_fb) = self.logic.write().on_feedback(self.timer.now_ms(), self.channel, FeedbackConsumerId::Local(self.uuid), fb) {
            self.local.read().feedback(self.channel.uuid(), local_fb);
        }
    }

    pub async fn recv(&self) -> Option<(LocalSubId, NodeId, ChannelUuid, Bytes)> {
        self.rx.recv().await.ok()
    }
}

impl Drop for ConsumerSingle {
    fn drop(&mut self) {
        self.logic.write().on_local_unsub(self.channel, self.uuid);
        self.local.write().on_local_unsub(self.uuid);
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use p_8xff_sdn_utils::MockTimer;
    use parking_lot::RwLock;

    use crate::{
        relay::{local::LocalRelay, logic::PubsubRelayLogic},
        ChannelIdentify, ConsumerSingle,
    };

    #[test]
    fn correct_create_and_destroy() {
        let node_id = 1;
        let channel = 1111;
        let channel_source = 2;

        let logic = Arc::new(RwLock::new(PubsubRelayLogic::new(node_id)));
        let local = Arc::new(RwLock::new(LocalRelay::new()));
        let timer = Arc::new(MockTimer::default());
        let sub_uuid = 10000;
        let consumer = ConsumerSingle::new(sub_uuid, ChannelIdentify::new(channel, channel_source), logic, local, 100, timer);

        assert_eq!(
            consumer.logic.read().relay(ChannelIdentify::new(channel, channel_source)),
            Some((vec![].as_slice(), vec![sub_uuid].as_slice()))
        );

        let logic = consumer.logic.clone();
        drop(consumer);

        assert_eq!(logic.read().relay(ChannelIdentify::new(channel, channel_source)), None);
    }
}

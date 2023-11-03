use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    sync::Arc,
};

use async_std::channel::Sender;
use bluesea_identity::NodeId;
use bytes::Bytes;
use utils::{awaker::Awaker, error_handle::ErrorUtils};

use super::{feedback::Feedback, ChannelUuid, LocalSubId};

#[derive(Debug, PartialEq, Eq)]
pub enum LocalRelayAction {
    Publish(ChannelUuid),
    Unpublish(ChannelUuid),
}

pub struct LocalRelay {
    consumers: HashMap<u64, Sender<(LocalSubId, NodeId, ChannelUuid, Bytes)>>,
    producer_fbs: HashMap<ChannelUuid, HashMap<u64, Sender<Feedback>>>,
    actions: VecDeque<LocalRelayAction>,
    awaker: Arc<dyn Awaker>,
}

impl LocalRelay {
    pub fn new() -> Self {
        Self {
            consumers: HashMap::new(),
            producer_fbs: HashMap::new(),
            actions: VecDeque::new(),
            awaker: Arc::new(utils::awaker::MockAwaker::default()),
        }
    }

    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.awaker = awaker;
    }

    pub fn on_local_sub(&mut self, uuid: LocalSubId, sender: Sender<(LocalSubId, NodeId, ChannelUuid, Bytes)>) {
        self.consumers.insert(uuid, sender);
    }

    pub fn on_local_unsub(&mut self, uuid: LocalSubId) {
        self.consumers.remove(&uuid);
    }

    pub fn on_local_pub(&mut self, channel: ChannelUuid, local_uuid: u64, fb_sender: Sender<Feedback>) {
        match self.producer_fbs.entry(channel) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().insert(local_uuid, fb_sender);
            }
            Entry::Vacant(entry) => {
                entry.insert(HashMap::from([(local_uuid, fb_sender)]));
                self.actions.push_back(LocalRelayAction::Publish(channel));
                self.awaker.notify();
            }
        }
    }

    pub fn on_local_unpub(&mut self, channel: ChannelUuid, local_uuid: u64) {
        if let Some(entry) = self.producer_fbs.get_mut(&channel) {
            entry.remove(&local_uuid);
            if entry.is_empty() {
                self.producer_fbs.remove(&channel);
                self.actions.push_back(LocalRelayAction::Unpublish(channel));
                self.awaker.notify();
            }
        }
    }

    pub fn feedback(&self, uuid: ChannelUuid, fb: Feedback) {
        if let Some(senders) = self.producer_fbs.get(&uuid) {
            for (_, sender) in senders {
                sender.try_send(fb.clone()).print_error("Should send feedback");
            }
        }
    }

    pub fn relay(&self, source: NodeId, channel: ChannelUuid, locals: &[LocalSubId], data: Bytes) {
        for uuid in locals {
            if let Some(sender) = self.consumers.get(uuid) {
                log::trace!("[LocalRelay] relay to local {}", uuid);
                sender.try_send((*uuid, source, channel, data.clone())).print_error("Should send data");
            } else {
                log::warn!("[LocalRelay] relay channel {} from {} to local {} consumer not found", channel, source, uuid);
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<LocalRelayAction> {
        self.actions.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use utils::awaker::Awaker;

    use crate::{
        relay::{feedback::FeedbackType, local::LocalRelayAction},
        ChannelIdentify,
    };

    #[test]
    fn first_pub_should_awake_and_output_action() {
        let awake = Arc::new(utils::awaker::MockAwaker::default());
        let mut relay = super::LocalRelay::new();
        relay.set_awaker(awake.clone());

        let (tx, _rx) = async_std::channel::bounded(1);
        relay.on_local_pub(1, 10, tx.clone());

        assert_eq!(awake.pop_awake_count(), 1);
        assert_eq!(relay.pop_action(), Some(LocalRelayAction::Publish(1)));
        assert_eq!(relay.pop_action(), None);

        relay.on_local_pub(1, 11, tx);
        assert_eq!(awake.pop_awake_count(), 0);
        assert_eq!(relay.pop_action(), None);

        relay.on_local_unpub(1, 10);
        assert_eq!(awake.pop_awake_count(), 0);
        assert_eq!(relay.pop_action(), None);

        relay.on_local_unpub(1, 11);
        assert_eq!(awake.pop_awake_count(), 1);
        assert_eq!(relay.pop_action(), Some(LocalRelayAction::Unpublish(1)));
    }

    #[test]
    fn should_relay_to_all_consumers() {
        let mut relay = super::LocalRelay::new();

        let (tx1, rx1) = async_std::channel::bounded(1);
        let (tx2, rx2) = async_std::channel::bounded(1);

        relay.on_local_sub(10, tx1);
        relay.on_local_sub(11, tx2);

        let data1 = Bytes::from("hello1");
        relay.relay(1, 1000, &[10, 11], data1.clone());
        assert_eq!(rx1.try_recv(), Ok((10, 1, 1000, data1.clone())));
        assert_eq!(rx2.try_recv(), Ok((11, 1, 1000, data1.clone())));

        let data2 = Bytes::from("hello2");
        let data3 = Bytes::from("hello2");
        relay.relay(1, 1000, &[10], data2.clone());
        relay.relay(1, 1000, &[11], data3.clone());
        assert_eq!(rx1.try_recv(), Ok((10, 1, 1000, data2.clone())));
        assert_eq!(rx2.try_recv(), Ok((11, 1, 1000, data3.clone())));
    }

    #[test]
    fn should_feedback_to_all_publishers() {
        let mut relay = super::LocalRelay::new();

        let (tx1, rx1) = async_std::channel::bounded(1);
        let (tx2, rx2) = async_std::channel::bounded(1);

        relay.on_local_pub(1, 10, tx1);
        relay.on_local_pub(1, 11, tx2);

        let channel = ChannelIdentify::new(1, 1);
        let fb = super::Feedback {
            channel,
            id: 1,
            feedback_type: FeedbackType::Passthrough(vec![1]),
        };

        relay.feedback(1, fb.clone());
        assert_eq!(rx1.try_recv(), Ok(fb.clone()));
        assert_eq!(rx2.try_recv(), Ok(fb.clone()));
    }
}

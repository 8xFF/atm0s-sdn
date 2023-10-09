use std::collections::HashMap;

use async_std::channel::Sender;
use bytes::Bytes;
use utils::error_handle::ErrorUtils;

use super::{feedback::Feedback, ChannelUuid, LocalPubId, LocalSubId};

#[derive(Clone)]
pub struct LocalRelay {
    consumers: HashMap<u64, Sender<Bytes>>,
    producer_fbs: HashMap<ChannelUuid, Sender<Feedback>>,
}

impl LocalRelay {
    pub fn new() -> Self {
        Self {
            consumers: HashMap::new(),
            producer_fbs: HashMap::new(),
        }
    }

    pub fn on_local_sub(&mut self, uuid: LocalSubId, sender: Sender<Bytes>) {
        self.consumers.insert(uuid, sender);
    }

    pub fn on_local_unsub(&mut self, uuid: LocalSubId) {
        self.consumers.remove(&uuid);
    }

    pub fn on_local_pub(&mut self, uuid: ChannelUuid, fb_sender: Sender<Feedback>) {
        self.producer_fbs.insert(uuid, fb_sender);
    }

    pub fn on_local_unpub(&mut self, uuid: ChannelUuid) {
        self.producer_fbs.remove(&uuid);
    }

    pub fn feedback(&self, uuid: ChannelUuid, fb: Feedback) {
        if let Some(sender) = self.producer_fbs.get(&uuid) {
            sender.try_send(fb).print_error("Should send feedback");
        }
    }

    pub fn relay(&self, locals: &[LocalSubId], data: Bytes) {
        for uuid in locals {
            if let Some(sender) = self.consumers.get(uuid) {
                sender.try_send(data.clone()).print_error("Should send data");
            }
        }
    }
}

use std::net::SocketAddr;

use crate::{
    base::FeatureControlActor,
    features::pubsub::msg::{Feedback, RelayControl},
};

use super::{consumers::RelayConsumers, feedbacks::FeedbacksAggerator, GenericRelay, GenericRelayOutput};

#[derive(Default)]
pub struct LocalRelay {
    consumers: RelayConsumers,
    feedbacks: FeedbacksAggerator,
    publishers: Vec<FeatureControlActor>,
}

impl GenericRelay for LocalRelay {
    fn on_tick(&mut self, now: u64) {
        self.feedbacks.on_tick(now);
        self.consumers.on_tick(now);
    }

    fn on_pub_start(&mut self, actor: FeatureControlActor) {
        if !self.publishers.contains(&actor) {
            log::debug!("[LocalRelay] on_pub_start {:?}", actor);
            self.publishers.push(actor);
        }
    }

    fn on_pub_stop(&mut self, actor: FeatureControlActor) {
        if let Some(index) = self.publishers.iter().position(|x| *x == actor) {
            log::debug!("[LocalRelay] on_pub_stop {:?}", actor);
            self.publishers.swap_remove(index);
        }
    }

    fn on_local_sub(&mut self, now: u64, actor: FeatureControlActor) {
        self.consumers.on_local_sub(now, actor);
    }

    fn on_local_feedback(&mut self, now: u64, actor: FeatureControlActor, feedback: Feedback) {
        self.feedbacks.on_local_feedback(now, actor, feedback);
    }

    fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor) {
        self.consumers.on_local_unsub(now, actor);
    }

    fn on_remote(&mut self, now: u64, remote: SocketAddr, control: RelayControl) {
        if let RelayControl::Feedback(fb) = control {
            self.feedbacks.on_remote_feedback(now, remote, fb);
        } else {
            self.consumers.on_remote(now, remote, control);
        }
    }

    fn conn_disconnected(&mut self, now: u64, remote: SocketAddr) {
        self.consumers.conn_disconnected(now, remote);
    }

    fn should_clear(&self) -> bool {
        self.consumers.should_clear() && self.publishers.is_empty()
    }

    fn relay_dests(&self) -> Option<(&[FeatureControlActor], bool)> {
        Some(self.consumers.relay_dests())
    }

    fn pop_output(&mut self) -> Option<GenericRelayOutput> {
        if let Some(fb) = self.feedbacks.pop_output() {
            log::debug!("[LocalRelay] pop_output feedback {:?}", fb);
            return Some(GenericRelayOutput::Feedback(self.publishers.clone(), fb));
        }
        self.consumers.pop_output().map(GenericRelayOutput::ToWorker)
    }
}

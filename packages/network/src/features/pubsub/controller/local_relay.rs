use std::{fmt::Debug, net::SocketAddr};

use derivative::Derivative;

use crate::{
    base::FeatureControlActor,
    features::pubsub::msg::{Feedback, RelayControl},
};

use super::{consumers::RelayConsumers, feedbacks::FeedbacksAggerator, GenericRelay, GenericRelayOutput};

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
pub struct LocalRelay<UserData> {
    consumers: RelayConsumers<UserData>,
    feedbacks: FeedbacksAggerator<UserData>,
    publishers: Vec<FeatureControlActor<UserData>>,
}

impl<UserData: Eq + Debug + Copy> GenericRelay<UserData> for LocalRelay<UserData> {
    fn on_tick(&mut self, now: u64) {
        self.feedbacks.on_tick(now);
        self.consumers.on_tick(now);
    }

    fn on_pub_start(&mut self, actor: FeatureControlActor<UserData>) {
        if !self.publishers.contains(&actor) {
            log::debug!("[LocalRelay] on_pub_start {:?}", actor);
            self.publishers.push(actor);
        }
    }

    fn on_pub_stop(&mut self, actor: FeatureControlActor<UserData>) {
        if let Some(index) = self.publishers.iter().position(|x| *x == actor) {
            log::debug!("[LocalRelay] on_pub_stop {:?}", actor);
            self.publishers.swap_remove(index);
        }
    }

    fn on_local_sub(&mut self, now: u64, actor: FeatureControlActor<UserData>) {
        self.consumers.on_local_sub(now, actor);
    }

    fn on_local_feedback(&mut self, now: u64, actor: FeatureControlActor<UserData>, feedback: Feedback) {
        self.feedbacks.on_local_feedback(now, actor, feedback);
    }

    fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor<UserData>) {
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

    fn relay_dests(&self) -> Option<(&[FeatureControlActor<UserData>], bool)> {
        Some(self.consumers.relay_dests())
    }

    fn pop_output(&mut self) -> Option<GenericRelayOutput<UserData>> {
        if let Some(fb) = self.feedbacks.pop_output() {
            log::debug!("[LocalRelay] pop_output feedback {:?}", fb);
            return Some(GenericRelayOutput::Feedback(self.publishers.clone(), fb));
        }
        self.consumers.pop_output().map(GenericRelayOutput::ToWorker)
    }
}

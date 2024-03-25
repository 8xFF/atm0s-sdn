use crate::{base::FeatureControlActor, features::pubsub::msg::RelayControl};

use super::{consumers::RelayConsumers, GenericRelay, GenericRelayOutput};

#[derive(Default)]
pub struct LocalRelay {
    consumers: RelayConsumers,
}

impl GenericRelay for LocalRelay {
    fn on_tick(&mut self, now: u64) {
        self.consumers.on_tick(now);
    }

    fn on_local_sub(&mut self, now: u64, actor: FeatureControlActor) {
        self.consumers.on_local_sub(now, actor);
    }

    fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor) {
        self.consumers.on_local_unsub(now, actor);
    }

    fn on_remote(&mut self, now: u64, remote: std::net::SocketAddr, control: RelayControl) {
        self.consumers.on_remote(now, remote, control);
    }

    fn conn_disconnected(&mut self, now: u64, remote: std::net::SocketAddr) {
        self.consumers.conn_disconnected(now, remote);
    }

    fn should_clear(&self) -> bool {
        self.consumers.should_clear()
    }

    fn relay_dests(&self) -> Option<(&[FeatureControlActor], bool)> {
        Some(self.consumers.relay_dests())
    }

    fn pop_output(&mut self) -> Option<GenericRelayOutput> {
        self.consumers.pop_output().map(GenericRelayOutput::ToWorker)
    }
}

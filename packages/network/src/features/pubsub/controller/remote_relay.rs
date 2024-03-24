//! This is single relay logic, which take care relay both local and remotes of a smallest part in pubsub sytem: Channel from a single source
//! This part take care two things:
//!
//! - Sub, Unsub logic
//! - Generate sync route table control message

use crate::{
    base::FeatureControlActor,
    features::pubsub::{msg::RelayControl, RelayWorkerControl},
};
use std::{collections::VecDeque, net::SocketAddr};

use super::{consumers::RelayConsummers, GenericRelay, RELAY_TIMEOUT};

enum RelayState {
    New,
    Binding { consumers: RelayConsummers },
    Bound { consumers: RelayConsummers, next: SocketAddr },
    Unbinding { next: SocketAddr, started_at: u64 },
    Unbound,
}

pub struct RemoteRelay {
    uuid: u64,
    state: RelayState,
    queue: VecDeque<RelayWorkerControl>,
}

impl RemoteRelay {
    pub fn new(uuid: u64) -> Self {
        Self {
            uuid,
            state: RelayState::New,
            queue: VecDeque::new(),
        }
    }

    fn pop_consumers_out(consumers: &mut RelayConsummers, queue: &mut VecDeque<RelayWorkerControl>) {
        while let Some(control) = consumers.pop_output() {
            queue.push_back(control);
        }
    }
}

impl GenericRelay for RemoteRelay {
    fn on_tick(&mut self, now: u64) {
        match &mut self.state {
            RelayState::Bound { next, consumers, .. } => {
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, Some(*next)));
                consumers.on_tick(now);
                Self::pop_consumers_out(consumers, &mut self.queue);

                if consumers.should_clear() {
                    self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                    self.state = RelayState::Unbinding { next: *next, started_at: now };
                }
            }
            RelayState::Binding { consumers, .. } => {
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
                consumers.on_tick(now);
                Self::pop_consumers_out(consumers, &mut self.queue);

                if consumers.should_clear() {
                    self.state = RelayState::Unbound;
                }
            }
            RelayState::Unbinding { next, started_at } => {
                if now >= *started_at + RELAY_TIMEOUT {
                    self.state = RelayState::Unbound;
                } else {
                    self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                }
            }
            _ => {}
        }
    }

    fn conn_disconnected(&mut self, now: u64, remote: SocketAddr) {
        match &mut self.state {
            RelayState::Bound { consumers, next, .. } => {
                consumers.conn_disconnected(now, remote);
                Self::pop_consumers_out(consumers, &mut self.queue);
                if *next == remote {
                    let consumers = std::mem::replace(consumers, Default::default());
                    self.state = RelayState::Binding { consumers };
                    self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
                } else if consumers.should_clear() {
                    self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                    self.state = RelayState::Unbinding { next: *next, started_at: now };
                }
            }
            RelayState::Binding { consumers, .. } => {
                consumers.conn_disconnected(now, remote);
                Self::pop_consumers_out(consumers, &mut self.queue);

                if consumers.should_clear() {
                    self.state = RelayState::Unbound;
                }
            }
            _ => {}
        }
    }

    /// Add a local subscriber to the relay
    /// Returns true if this is the first subscriber, false otherwise
    fn on_local_sub(&mut self, now: u64, actor: FeatureControlActor) {
        match &mut self.state {
            RelayState::New | RelayState::Unbound => {
                let mut consumers = RelayConsummers::default();
                consumers.on_local_sub(now, actor);
                Self::pop_consumers_out(&mut consumers, &mut self.queue);
                self.state = RelayState::Binding { consumers };
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
            }
            RelayState::Unbinding { next, .. } => {
                let mut consumers = RelayConsummers::default();
                consumers.on_local_sub(now, actor);
                Self::pop_consumers_out(&mut consumers, &mut self.queue);
                self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, Some(*next)));
                self.state = RelayState::Binding { consumers };
            }
            RelayState::Binding { consumers, .. } | RelayState::Bound { consumers, .. } => {
                consumers.on_local_sub(now, actor);
                Self::pop_consumers_out(consumers, &mut self.queue);
            }
        }
    }

    /// Remove a local subscriber from the relay
    /// Returns true if this is the last subscriber, false otherwise
    fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor) {
        match &mut self.state {
            RelayState::New | RelayState::Unbound => {
                log::warn!("[Relay] Unsub for unknown relay {:?}", self.uuid);
            }
            RelayState::Unbinding { .. } => {
                log::warn!("[Relay] Unsub for relay in unbinding state {:?}", self.uuid);
            }
            RelayState::Binding { consumers, .. } => {
                consumers.on_local_unsub(now, actor);
                Self::pop_consumers_out(consumers, &mut self.queue);
                if consumers.should_clear() {
                    self.state = RelayState::Unbound;
                }
            }
            RelayState::Bound { next, consumers, .. } => {
                consumers.on_local_unsub(now, actor);
                Self::pop_consumers_out(consumers, &mut self.queue);
                if consumers.should_clear() {
                    self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                    self.state = RelayState::Unbinding { next: *next, started_at: now };
                }
            }
        }
    }

    fn on_remote(&mut self, now: u64, remote: SocketAddr, control: RelayControl) {
        match control {
            RelayControl::SubOK(uuid) => {
                if uuid != self.uuid {
                    log::warn!("[Relay] SubOK for wrong relay session {uuid} vs {}", uuid);
                    return;
                }
                match &mut self.state {
                    RelayState::Binding { consumers } => {
                        log::info!("[Relay] SubOK for binding relay {} from {remote} => switched to Bound with this remote", self.uuid);
                        let consumers = std::mem::replace(consumers, Default::default());
                        self.state = RelayState::Bound { next: remote, consumers };
                    }
                    _ => {}
                }
            }
            RelayControl::UnsubOK(uuid) => {
                if uuid != self.uuid {
                    log::warn!("[Relay] UnsubOK for wrong relay session {uuid} vs {}", uuid);
                    return;
                }
                match &mut self.state {
                    RelayState::Unbinding { .. } => {
                        log::info!("[Relay] UnsubOK for unbinding relay {} from {remote} => switched to Unbound", self.uuid);
                        self.state = RelayState::Unbound;
                    }
                    _ => {}
                }
            }
            _ => match &mut self.state {
                RelayState::New | RelayState::Unbound => {
                    let mut consumers = RelayConsummers::default();
                    consumers.on_remote(now, remote, control);
                    Self::pop_consumers_out(&mut consumers, &mut self.queue);
                    self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, None));
                    self.state = RelayState::Binding { consumers };
                }
                RelayState::Unbinding { next, .. } => {
                    let mut consumers = RelayConsummers::default();
                    consumers.on_remote(now, remote, control);
                    Self::pop_consumers_out(&mut consumers, &mut self.queue);
                    self.queue.push_back(RelayWorkerControl::SendSub(self.uuid, Some(*next)));
                    self.state = RelayState::Binding { consumers };
                }
                RelayState::Binding { consumers, .. } => {
                    consumers.on_remote(now, remote, control);
                    Self::pop_consumers_out(consumers, &mut self.queue);
                    if consumers.should_clear() {
                        self.state = RelayState::Unbound;
                    }
                }
                RelayState::Bound { consumers, next, .. } => {
                    consumers.on_remote(now, remote, control);
                    Self::pop_consumers_out(consumers, &mut self.queue);
                    if consumers.should_clear() {
                        self.queue.push_back(RelayWorkerControl::SendUnsub(self.uuid, *next));
                        self.state = RelayState::Unbinding { next: *next, started_at: now };
                    }
                }
            },
        }
    }

    fn pop_output(&mut self) -> Option<RelayWorkerControl> {
        self.queue.pop_front()
    }

    fn relay_dests(&self) -> Option<(&[FeatureControlActor], bool)> {
        match &self.state {
            RelayState::Bound { consumers, .. } | RelayState::Binding { consumers, .. } => Some(consumers.relay_dests()),
            _ => None,
        }
    }

    fn should_clear(&self) -> bool {
        matches!(self.state, RelayState::Unbound)
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use crate::{
        base::FeatureControlActor,
        features::pubsub::{
            controller::{GenericRelay, RELAY_TIMEOUT},
            msg::RelayControl,
            RelayWorkerControl,
        },
    };

    use super::RemoteRelay;

    #[test]
    fn on_local_sub_unsub() {
        let mut relay = RemoteRelay::new(1000);

        relay.on_local_sub(100, FeatureControlActor::Controller);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));

        relay.on_local_unsub(300, FeatureControlActor::Controller);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Controller)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendUnsub(1000, remote)));
        assert_eq!(relay.pop_output(), None);

        assert!(!relay.should_clear());

        //fake UnsubOk
        relay.on_remote(400, remote, RelayControl::UnsubOK(1000));

        assert!(relay.should_clear());
    }

    #[test]
    fn on_remote_sub_unsub() {
        let mut relay = RemoteRelay::new(1000);

        let consumer = SocketAddr::from(([127, 0, 0, 1], 1000));

        relay.on_remote(100, consumer, RelayControl::Sub(2000));

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSubOk(2000, consumer)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteSetRemote(consumer)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));

        relay.on_remote(300, consumer, RelayControl::Unsub(2000));

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendUnsubOk(2000, consumer)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteDelRemote(consumer)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendUnsub(1000, remote)));
        assert_eq!(relay.pop_output(), None);

        assert!(!relay.should_clear());

        //fake UnsubOk
        relay.on_remote(400, remote, RelayControl::UnsubOK(1000));

        assert!(relay.should_clear());
    }

    #[test]
    fn retry_sending_sub() {
        let mut relay = RemoteRelay::new(1000);

        relay.on_local_sub(100, FeatureControlActor::Controller);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);

        relay.on_tick(200);
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);

        relay.on_tick(300);
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);
    }

    #[test]
    fn retry_sending_unsub() {
        let mut relay = RemoteRelay::new(1000);

        relay.on_local_sub(100, FeatureControlActor::Controller);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));

        relay.on_local_unsub(300, FeatureControlActor::Controller);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Controller)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendUnsub(1000, remote)));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        relay.on_tick(200);
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendUnsub(1000, remote)));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        relay.on_tick(300);
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendUnsub(1000, remote)));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        relay.on_tick(300 + RELAY_TIMEOUT);
        assert_eq!(relay.pop_output(), None);
        assert!(relay.should_clear());
    }

    #[test]
    fn retry_sending_sub_after_disconnected_to_next() {
        let mut relay = RemoteRelay::new(1000);

        relay.on_local_sub(100, FeatureControlActor::Controller);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));

        //simulate next is disconnected
        relay.conn_disconnected(300, remote);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);
    }

    #[test]
    fn consumer_disconnected_should_unsub_if_empty() {
        let mut relay = RemoteRelay::new(1000);

        let consumer = SocketAddr::from(([127, 0, 0, 1], 1000));

        relay.on_remote(100, consumer, RelayControl::Sub(2000));

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSubOk(2000, consumer)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteSetRemote(consumer)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendSub(1000, None)));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));

        //simulate consumer is disconnected
        relay.conn_disconnected(300, consumer);

        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::RouteDelRemote(consumer)));
        assert_eq!(relay.pop_output(), Some(RelayWorkerControl::SendUnsub(1000, remote)));
        assert_eq!(relay.pop_output(), None);
    }
}

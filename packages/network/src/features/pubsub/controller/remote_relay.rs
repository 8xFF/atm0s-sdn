//! This is single relay logic, which take care relay both local and remotes of a smallest part in pubsub system: Channel from a single source
//! This part take care two things:
//!
//! - Sub, Unsub logic
//! - Generate sync route table control message

use crate::{
    base::FeatureControlActor,
    features::pubsub::{
        msg::{Feedback, RelayControl},
        RelayWorkerControl,
    },
};
use std::{collections::VecDeque, fmt::Debug, net::SocketAddr};

use super::{consumers::RelayConsumers, feedbacks::FeedbacksAggerator, GenericRelay, GenericRelayOutput, RELAY_STICKY_MS, RELAY_TIMEOUT};

enum RelayState<UserData> {
    New,
    Binding {
        consumers: RelayConsumers<UserData>,
        feedbacks: FeedbacksAggerator<UserData>,
    },
    Bound {
        consumers: RelayConsumers<UserData>,
        feedbacks: FeedbacksAggerator<UserData>,
        next: SocketAddr,
        sticky_session_at: u64,
    },
    Unbinding {
        next: SocketAddr,
        started_at: u64,
    },
    Unbound,
}

pub struct RemoteRelay<UserData> {
    uuid: u64,
    state: RelayState<UserData>,
    queue: VecDeque<GenericRelayOutput<UserData>>,
}

impl<UserData: Eq + Copy + Debug> RemoteRelay<UserData> {
    pub fn new(uuid: u64) -> Self {
        Self {
            uuid,
            state: RelayState::New,
            queue: VecDeque::new(),
        }
    }

    fn pop_consumers_out(consumers: &mut RelayConsumers<UserData>, queue: &mut VecDeque<GenericRelayOutput<UserData>>) {
        while let Some(control) = consumers.pop_output() {
            queue.push_back(GenericRelayOutput::ToWorker(control));
        }
    }

    fn pop_feedbacks_out(remote: SocketAddr, feedbacks: &mut FeedbacksAggerator<UserData>, queue: &mut VecDeque<GenericRelayOutput<UserData>>) {
        while let Some(fb) = feedbacks.pop_output() {
            queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendFeedback(fb, remote)));
        }
    }
}

impl<UserData: Eq + Copy + Debug> GenericRelay<UserData> for RemoteRelay<UserData> {
    fn on_tick(&mut self, now: u64) {
        match &mut self.state {
            RelayState::Bound {
                next,
                consumers,
                feedbacks,
                sticky_session_at,
                ..
            } => {
                if now >= *sticky_session_at + RELAY_STICKY_MS {
                    log::info!("[PubSubRemoteRelay] Sticky session end for relay from {next} => trying finding better way");
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, None)));
                } else {
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, Some(*next))));
                }
                consumers.on_tick(now);
                feedbacks.on_tick(now);
                Self::pop_consumers_out(consumers, &mut self.queue);
                Self::pop_feedbacks_out(*next, feedbacks, &mut self.queue);

                if consumers.should_clear() {
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(self.uuid, *next)));
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(*next)));
                    self.state = RelayState::Unbinding { next: *next, started_at: now };
                }
            }
            RelayState::Binding { consumers, .. } => {
                self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, None)));
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
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(self.uuid, *next)));
                }
            }
            _ => {}
        }
    }

    fn conn_disconnected(&mut self, now: u64, remote: SocketAddr) {
        match &mut self.state {
            RelayState::Bound { consumers, feedbacks, next, .. } => {
                consumers.conn_disconnected(now, remote);
                Self::pop_consumers_out(consumers, &mut self.queue);
                // If remote is next, this will not be consumers, because it will cause loop deps
                if *next == remote {
                    let consumers = std::mem::replace(consumers, Default::default());
                    let feedbacks = std::mem::replace(feedbacks, Default::default());
                    self.state = RelayState::Binding { consumers, feedbacks };
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, None)));
                } else if consumers.should_clear() {
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(self.uuid, *next)));
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(*next)));
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

    fn on_pub_start(&mut self, _actor: FeatureControlActor<UserData>) {
        panic!("Should not be called");
    }

    fn on_pub_stop(&mut self, _actor: FeatureControlActor<UserData>) {
        panic!("Should not be called");
    }

    /// Add a local subscriber to the relay
    /// Returns true if this is the first subscriber, false otherwise
    fn on_local_sub(&mut self, now: u64, actor: FeatureControlActor<UserData>) {
        match &mut self.state {
            RelayState::New | RelayState::Unbound => {
                log::info!("[PubSubRemoteRelay] Sub in New or Unbound state => switch to Binding and send Sub message");
                let mut consumers = RelayConsumers::default();
                let feedbacks = FeedbacksAggerator::default();
                consumers.on_local_sub(now, actor);
                Self::pop_consumers_out(&mut consumers, &mut self.queue);
                self.state = RelayState::Binding { consumers, feedbacks };
                self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, None)));
            }
            RelayState::Unbinding { next, .. } => {
                log::debug!("[PubSubRemoteRelay] Sub in Unbinding state => switch to Binding with previous next {next}");
                let mut consumers = RelayConsumers::default();
                let feedbacks = FeedbacksAggerator::default();
                consumers.on_local_sub(now, actor);
                Self::pop_consumers_out(&mut consumers, &mut self.queue);
                self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, Some(*next))));
                self.state = RelayState::Binding { consumers, feedbacks };
            }
            RelayState::Binding { consumers, .. } | RelayState::Bound { consumers, .. } => {
                log::debug!("[PubSubRemoteRelay] Sub in Binding or Bound state => just add to list");
                consumers.on_local_sub(now, actor);
                Self::pop_consumers_out(consumers, &mut self.queue);
            }
        }
    }

    /// Sending feedback to sources, for avoiding wasting bandwidth, the feedback will be aggregated and send in each window_ms
    fn on_local_feedback(&mut self, now: u64, actor: FeatureControlActor<UserData>, feedback: Feedback) {
        match &mut self.state {
            RelayState::Binding { feedbacks, .. } => {
                feedbacks.on_local_feedback(now, actor, feedback);
            }
            RelayState::Bound { next, feedbacks, .. } => {
                feedbacks.on_local_feedback(now, actor, feedback);
                Self::pop_feedbacks_out(*next, feedbacks, &mut self.queue);
            }
            _ => {}
        }
    }

    /// Remove a local subscriber from the relay
    /// Returns true if this is the last subscriber, false otherwise
    fn on_local_unsub(&mut self, now: u64, actor: FeatureControlActor<UserData>) {
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
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(self.uuid, *next)));
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(*next)));
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
                    RelayState::Binding { consumers, feedbacks } => {
                        log::info!("[Relay] SubOK for binding relay {} from {remote} => switched to Bound with this remote", self.uuid);
                        self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetSource(remote)));
                        let consumers = std::mem::replace(consumers, Default::default());
                        let feedbacks = std::mem::replace(feedbacks, Default::default());
                        self.state = RelayState::Bound {
                            next: remote,
                            consumers,
                            feedbacks,
                            sticky_session_at: now,
                        };
                    }
                    RelayState::Bound {
                        next, sticky_session_at, consumers, ..
                    } => {
                        if *next == remote {
                            log::debug!("[Relay] SubOK for bound relay {} from same remote {remote} => renew sticky session", self.uuid);
                            *sticky_session_at = now;
                        } else {
                            log::warn!("[Relay] SubOK for bound relay {} from other remote {remote} => renew stick session and Unsub older", self.uuid);
                            let (locals, has_remote) = consumers.relay_dests();
                            self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(self.uuid, *next)));
                            if has_remote {
                                self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendRouteChanged));
                            }
                            self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetSource(remote)));
                            for actor in locals {
                                self.queue.push_back(GenericRelayOutput::RouteChanged(*actor));
                            }
                            *next = remote;
                        }
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
            RelayControl::RouteChanged(uuid) => {
                if uuid != self.uuid {
                    log::warn!("[Relay] RouteChanged for wrong relay session {uuid} vs {}", uuid);
                    return;
                }
                match &mut self.state {
                    RelayState::Bound { consumers, .. } => {
                        let (locals, has_remote) = consumers.relay_dests();
                        if has_remote {
                            self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendRouteChanged));
                        }
                        for actor in locals {
                            self.queue.push_back(GenericRelayOutput::RouteChanged(*actor));
                        }
                    }
                    _ => {}
                }
            }
            RelayControl::Feedback(fb) => match &mut self.state {
                RelayState::Binding { feedbacks, .. } => {
                    feedbacks.on_remote_feedback(now, remote, fb);
                }
                RelayState::Bound { next, feedbacks, .. } => {
                    feedbacks.on_remote_feedback(now, remote, fb);
                    Self::pop_feedbacks_out(*next, feedbacks, &mut self.queue);
                }
                _ => {}
            },
            _ => match &mut self.state {
                RelayState::New | RelayState::Unbound => {
                    let mut consumers = RelayConsumers::default();
                    let feedbacks = FeedbacksAggerator::default();
                    consumers.on_remote(now, remote, control);
                    Self::pop_consumers_out(&mut consumers, &mut self.queue);
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, None)));
                    self.state = RelayState::Binding { consumers, feedbacks };
                }
                RelayState::Unbinding { next, .. } => {
                    let mut consumers = RelayConsumers::default();
                    let feedbacks = FeedbacksAggerator::default();
                    consumers.on_remote(now, remote, control);
                    Self::pop_consumers_out(&mut consumers, &mut self.queue);
                    self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(self.uuid, Some(*next))));
                    self.state = RelayState::Binding { consumers, feedbacks };
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
                        self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(self.uuid, *next)));
                        self.queue.push_back(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(*next)));
                        self.state = RelayState::Unbinding { next: *next, started_at: now };
                    }
                }
            },
        }
    }

    fn pop_output(&mut self) -> Option<GenericRelayOutput<UserData>> {
        self.queue.pop_front()
    }

    fn relay_dests(&self) -> Option<(&[FeatureControlActor<UserData>], bool)> {
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
            controller::{GenericRelay, GenericRelayOutput, RELAY_STICKY_MS, RELAY_TIMEOUT},
            msg::RelayControl,
            RelayWorkerControl,
        },
    };

    use super::RemoteRelay;

    fn create_local_bound_relay(uuid: u64, actor: FeatureControlActor<()>, remote: SocketAddr) -> RemoteRelay<()> {
        let mut relay = RemoteRelay::new(uuid);

        relay.on_local_sub(0, actor);

        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetLocal(actor))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(uuid, None))));
        assert_eq!(relay.pop_output(), None);

        //fake SubOk
        relay.on_remote(0, remote, RelayControl::SubOK(uuid));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetSource(remote))));

        relay
    }

    #[test]
    fn on_local_sub_unsub() {
        let mut relay = RemoteRelay::new(1000);

        relay.on_local_sub(100, FeatureControlActor::Controller(()));

        assert_eq!(
            relay.pop_output(),
            Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller(()))))
        );
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetSource(remote))));

        relay.on_local_unsub(300, FeatureControlActor::Controller(()));

        assert_eq!(
            relay.pop_output(),
            Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Controller(()))))
        );
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(1000, remote))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(remote))));
        assert_eq!(relay.pop_output(), None);

        assert!(!relay.should_clear());

        //fake UnsubOk
        relay.on_remote(400, remote, RelayControl::UnsubOK(1000));

        assert!(relay.should_clear());
    }

    #[test]
    fn on_remote_sub_unsub() {
        let mut relay = RemoteRelay::<()>::new(1000);

        let consumer = SocketAddr::from(([127, 0, 0, 1], 1000));

        relay.on_remote(100, consumer, RelayControl::Sub(2000));

        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSubOk(2000, consumer))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetRemote(consumer, 2000))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetSource(remote))));

        relay.on_remote(300, consumer, RelayControl::Unsub(2000));

        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsubOk(2000, consumer))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelRemote(consumer))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(1000, remote))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(remote))));
        assert_eq!(relay.pop_output(), None);

        assert!(!relay.should_clear());

        //fake UnsubOk
        relay.on_remote(400, remote, RelayControl::UnsubOK(1000));

        assert!(relay.should_clear());
    }

    #[test]
    fn retry_sending_sub() {
        let mut relay = RemoteRelay::new(1000);

        relay.on_local_sub(100, FeatureControlActor::Controller(()));

        assert_eq!(
            relay.pop_output(),
            Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller(()))))
        );
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);

        relay.on_tick(200);
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);

        relay.on_tick(300);
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);
    }

    #[test]
    fn retry_sending_unsub() {
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        let mut relay = create_local_bound_relay(1000, FeatureControlActor::Controller(()), remote);

        relay.on_local_unsub(300, FeatureControlActor::Controller(()));

        assert_eq!(
            relay.pop_output(),
            Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Controller(()))))
        );
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(1000, remote))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(remote))));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        relay.on_tick(200);
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(1000, remote))));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        relay.on_tick(300);
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(1000, remote))));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        relay.on_tick(300 + RELAY_TIMEOUT);
        assert_eq!(relay.pop_output(), None);
        assert!(relay.should_clear());
    }

    #[test]
    fn retry_sending_sub_after_disconnected_to_next() {
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        let mut relay = create_local_bound_relay(1000, FeatureControlActor::Controller(()), remote);

        //simulate next is disconnected
        relay.conn_disconnected(300, remote);

        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);
    }

    #[test]
    fn consumer_disconnected_should_unsub_if_empty() {
        let mut relay = RemoteRelay::<()>::new(1000);

        let consumer = SocketAddr::from(([127, 0, 0, 1], 1000));

        relay.on_remote(100, consumer, RelayControl::Sub(2000));

        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSubOk(2000, consumer))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetRemote(consumer, 2000))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);
        assert!(!relay.should_clear());

        //fake SubOk
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        relay.on_remote(200, remote, RelayControl::SubOK(1000));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetSource(remote))));

        //simulate consumer is disconnected
        relay.conn_disconnected(300, consumer);

        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelRemote(consumer))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(1000, remote))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteDelSource(remote))));
        assert_eq!(relay.pop_output(), None);
    }

    #[test]
    fn sticky_session_timeout_should_fire_bind_without_remote_hint() {
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        let mut relay = create_local_bound_relay(1000, FeatureControlActor::Controller(()), remote);

        //default resend sub
        relay.on_tick(300);
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, Some(remote)))));
        assert_eq!(relay.pop_output(), None);

        //after timeout should bind without remote hint
        relay.on_tick(RELAY_STICKY_MS);
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);
    }

    #[test]
    fn sticky_session_timeout_bind_to_new_remote() {
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        let mut relay = create_local_bound_relay(1000, FeatureControlActor::Controller(()), remote);

        let remote2 = SocketAddr::from(([127, 0, 0, 2], 1234));

        //after timeout should bind without remote hint
        relay.on_tick(RELAY_STICKY_MS);
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendSub(1000, None))));
        assert_eq!(relay.pop_output(), None);

        //fake SubOk
        relay.on_remote(1 + RELAY_STICKY_MS, remote2, RelayControl::SubOK(1000));

        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::SendUnsub(1000, remote))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::ToWorker(RelayWorkerControl::RouteSetSource(remote2))));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::RouteChanged(FeatureControlActor::Controller(()))));
        assert_eq!(relay.pop_output(), None);
    }

    #[test]
    fn next_node_notify_route_changed() {
        let remote = SocketAddr::from(([127, 0, 0, 1], 1234));
        let mut relay = create_local_bound_relay(1000, FeatureControlActor::Controller(()), remote);

        //fake RouteChanged
        relay.on_remote(100, remote, RelayControl::RouteChanged(1000));
        assert_eq!(relay.pop_output(), Some(GenericRelayOutput::RouteChanged(FeatureControlActor::Controller(()))));
        assert_eq!(relay.pop_output(), None);
    }
}

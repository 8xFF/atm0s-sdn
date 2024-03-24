use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
};

use crate::{
    base::FeatureControlActor,
    features::pubsub::{msg::RelayControl, RelayWorkerControl},
};

use super::RELAY_TIMEOUT;

struct RelayRemote {
    uuid: u64,
    last_sub: u64,
}

#[derive(Default)]
pub struct RelayConsummers {
    remotes: HashMap<SocketAddr, RelayRemote>,
    locals: Vec<FeatureControlActor>,
    queue: VecDeque<RelayWorkerControl>,
}

impl RelayConsummers {
    pub fn on_tick(&mut self, now: u64) {
        let mut timeout = vec![];
        for (remote, slot) in self.remotes.iter() {
            if now >= slot.last_sub + RELAY_TIMEOUT {
                timeout.push(*remote);
            }
        }

        for remote in timeout {
            self.queue.push_back(RelayWorkerControl::RouteDelRemote(remote));
            self.remotes.remove(&remote);
        }
    }

    pub fn on_local_sub(&mut self, _now: u64, actor: FeatureControlActor) {
        if self.locals.contains(&actor) {
            log::warn!("[RelayConsumers] Sub for already subbed local actor {:?}", actor);
            return;
        }
        log::debug!("[RelayConsumers] Sub for local actor {:?}", actor);
        self.locals.push(actor);
        self.queue.push_back(RelayWorkerControl::RouteSetLocal(actor));
    }

    pub fn on_local_unsub(&mut self, _now: u64, actor: FeatureControlActor) {
        if let Some(pos) = self.locals.iter().position(|a| *a == actor) {
            log::debug!("[RelayConsumers] Unsub from local actor {:?}", actor);
            self.queue.push_back(RelayWorkerControl::RouteDelLocal(actor));
            self.locals.swap_remove(pos);
        } else {
            log::warn!("[Relay] Unsub for unknown local actor {:?}", actor);
        }
    }

    pub fn on_remote(&mut self, now: u64, remote: SocketAddr, control: RelayControl) {
        match control {
            RelayControl::Sub(uuid) => {
                if let Some(slot) = self.remotes.get_mut(&remote) {
                    if uuid == slot.uuid {
                        slot.last_sub = now;
                        self.queue.push_back(RelayWorkerControl::SendSubOk(uuid, remote));
                    } else {
                        log::warn!("[RelayConsumers] Sub for remote {remote} with different uuid {uuid} vs {}", slot.uuid);
                    }
                } else {
                    log::debug!("[PubSubConsumers] Sub for remote {remote} with uuid {uuid}");
                    self.remotes.insert(remote, RelayRemote { uuid, last_sub: now });
                    self.queue.push_back(RelayWorkerControl::SendSubOk(uuid, remote));
                    self.queue.push_back(RelayWorkerControl::RouteSetRemote(remote, uuid));
                }
            }
            RelayControl::Unsub(uuid) => {
                if let Some(slot) = self.remotes.get(&remote) {
                    if slot.uuid == uuid {
                        log::debug!("[PubSubConsumers] Unsub from remote {remote} with uuid {uuid}");
                        self.queue.push_back(RelayWorkerControl::SendUnsubOk(uuid, remote));
                        self.queue.push_back(RelayWorkerControl::RouteDelRemote(remote));
                        self.remotes.remove(&remote);
                    } else {
                        log::warn!("[Relay] Unsub for wrong session remote {remote}, {uuid} vs {}", slot.uuid);
                    }
                } else {
                    log::warn!("[Relay] Unsub for unknown remote {:?}", remote);
                }
            }
            _ => {}
        }
    }

    pub fn conn_disconnected(&mut self, _now: u64, remote: SocketAddr) {
        if let Some(_) = self.remotes.remove(&remote) {
            self.queue.push_back(RelayWorkerControl::RouteDelRemote(remote));
        }
    }

    pub fn should_clear(&self) -> bool {
        self.locals.is_empty() && self.remotes.is_empty()
    }

    pub fn relay_dests(&self) -> (&[FeatureControlActor], bool) {
        (self.locals.as_slice(), !self.remotes.is_empty())
    }

    pub fn pop_output(&mut self) -> Option<RelayWorkerControl> {
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use crate::{
        base::FeatureControlActor,
        features::pubsub::{msg::RelayControl, RelayWorkerControl},
    };

    use super::RelayConsummers;

    #[test]
    fn relay_local_should_work_single_sub() {
        let mut consumers = RelayConsummers::default();
        consumers.on_local_sub(0, FeatureControlActor::Controller);

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[FeatureControlActor::Controller] as &[_], false));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_local_unsub(100, FeatureControlActor::Controller);

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Controller)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], false));
        assert_eq!(consumers.should_clear(), true);
    }

    #[test]
    fn relay_local_should_work_multi_subs() {
        let mut consumers = RelayConsummers::default();
        consumers.on_local_sub(0, FeatureControlActor::Controller);

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller)));
        assert_eq!(consumers.pop_output(), None);

        consumers.on_local_sub(0, FeatureControlActor::Service(1.into()));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Service(1.into()))));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[FeatureControlActor::Controller, FeatureControlActor::Service(1.into())] as &[_], false));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_local_unsub(100, FeatureControlActor::Controller);

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Controller)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[FeatureControlActor::Service(1.into())] as &[_], false));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_local_unsub(100, FeatureControlActor::Service(1.into()));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Service(1.into()))));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], false));
        assert_eq!(consumers.should_clear(), true);
    }

    #[test]
    fn relay_remote_should_work_single_sub() {
        let mut consumers = RelayConsummers::default();

        let remote = SocketAddr::from(([127, 0, 0, 1], 8080));

        consumers.on_remote(0, remote, RelayControl::Sub(1000));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendSubOk(1000, remote)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetRemote(remote, 1000)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], true));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_remote(100, remote, RelayControl::Unsub(1000));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendUnsubOk(1000, remote)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelRemote(remote)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], false));
        assert_eq!(consumers.should_clear(), true);
    }

    #[test]
    fn relay_remote_should_work_multi_subs() {
        let mut consumers = RelayConsummers::default();

        let remote1 = SocketAddr::from(([127, 0, 0, 1], 8080));
        let remote2 = SocketAddr::from(([127, 0, 0, 2], 8080));

        consumers.on_remote(0, remote1, RelayControl::Sub(1000));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendSubOk(1000, remote1)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetRemote(remote1, 1000)));
        assert_eq!(consumers.pop_output(), None);

        consumers.on_remote(0, remote2, RelayControl::Sub(1001));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendSubOk(1001, remote2)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetRemote(remote2, 1001)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], true));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_remote(100, remote1, RelayControl::Unsub(1000));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendUnsubOk(1000, remote1)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelRemote(remote1)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], true));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_remote(200, remote2, RelayControl::Unsub(1001));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendUnsubOk(1001, remote2)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelRemote(remote2)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], false));
        assert_eq!(consumers.should_clear(), true);
    }

    #[test]
    fn relay_should_work_both_local_and_remote() {
        let mut consumers = RelayConsummers::default();

        let remote1 = SocketAddr::from(([127, 0, 0, 1], 8080));

        consumers.on_remote(0, remote1, RelayControl::Sub(1000));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendSubOk(1000, remote1)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetRemote(remote1, 1000)));
        assert_eq!(consumers.pop_output(), None);

        consumers.on_local_sub(0, FeatureControlActor::Controller);

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteSetLocal(FeatureControlActor::Controller)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[FeatureControlActor::Controller] as &[_], true));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_remote(100, remote1, RelayControl::Unsub(1000));

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::SendUnsubOk(1000, remote1)));
        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelRemote(remote1)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[FeatureControlActor::Controller] as &[_], false));
        assert_eq!(consumers.should_clear(), false);

        consumers.on_local_unsub(200, FeatureControlActor::Controller);

        assert_eq!(consumers.pop_output(), Some(RelayWorkerControl::RouteDelLocal(FeatureControlActor::Controller)));
        assert_eq!(consumers.pop_output(), None);

        assert_eq!(consumers.relay_dests(), (&[] as &[_], false));
        assert_eq!(consumers.should_clear(), true);
    }
}

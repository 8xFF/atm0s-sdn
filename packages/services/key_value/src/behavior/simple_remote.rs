use bluesea_identity::NodeId;
use bluesea_router::RouteRule;
use std::collections::VecDeque;
/// This remote storage is a simple key value storage, it will store all key value in memory, and send event to other node when key value changed
/// Each event is attached with a req_id and wait for ack, if ack not receive, it will resend the event each tick util ack received or tick_count is 0
use std::sync::Arc;
use utils::Timer;

use crate::storage::simple::{OutputEvent, SimpleKeyValue};
use crate::{
    msg::{LocalEvent, RemoteEvent},
    KeyId, ValueType,
};

use super::event_acks::EventAckManager;

const RETRY_COUNT: u8 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteStorageAction(pub(crate) LocalEvent, pub(crate) RouteRule);

pub struct RemoteStorage {
    req_id_seed: u64,
    storage: SimpleKeyValue<KeyId, ValueType, NodeId>,
    event_acks: EventAckManager<RemoteStorageAction>,
    output_events: VecDeque<RemoteStorageAction>,
}

impl RemoteStorage {
    pub fn new(timer: Arc<dyn Timer>) -> Self {
        Self {
            req_id_seed: 0,
            storage: SimpleKeyValue::new(timer),
            event_acks: EventAckManager::new(),
            output_events: VecDeque::new(),
        }
    }

    pub fn tick(&mut self) {
        self.storage.tick();
        self.event_acks.tick();
    }

    pub fn on_event(&mut self, from: NodeId, event: RemoteEvent) {
        match event {
            RemoteEvent::Set(req_id, key, value, version, ex) => {
                log::debug!("[RemoteStorage] receive set event from {} key {} value {:?} version {} ex {:?}", from, key, value, version, ex);
                let setted = self.storage.set(key, value, version, ex);
                self.output_events
                    .push_back(RemoteStorageAction(LocalEvent::SetAck(req_id, key, version, setted), RouteRule::ToNode(from)));
            }
            RemoteEvent::Get(req_id, key) => {
                if let Some((value, version)) = self.storage.get(&key) {
                    log::debug!("[RemoteStorage] receive get event from {} key {} value {:?} version {}", from, key, value, version);
                    self.output_events
                        .push_back(RemoteStorageAction(LocalEvent::GetAck(req_id, key, Some((value.clone(), version))), RouteRule::ToNode(from)));
                } else {
                    log::debug!("[RemoteStorage] receive get event from {} key {} value None", from, key);
                    self.output_events.push_back(RemoteStorageAction(LocalEvent::GetAck(req_id, key, None), RouteRule::ToNode(from)));
                }
            }
            RemoteEvent::Del(req_id, key, req_version) => {
                log::debug!("[RemoteStorage] receive del event from {} key {} version {:?}", from, key, req_version);
                let version = self.storage.del(&key, req_version).map(|(_, version)| version);
                self.output_events.push_back(RemoteStorageAction(LocalEvent::DelAck(req_id, key, version), RouteRule::ToNode(from)));
            }
            RemoteEvent::Sub(req_id, key, ex) => {
                log::debug!("[RemoteStorage] receive sub event from {} key {} ex {:?}", from, key, ex);
                self.storage.subscribe(&key, from, ex);
                self.output_events.push_back(RemoteStorageAction(LocalEvent::SubAck(req_id, key), RouteRule::ToNode(from)));
            }
            RemoteEvent::Unsub(req_id, key) => {
                log::debug!("[RemoteStorage] receive unsub event from {} key {}", from, key);
                let success = self.storage.unsubscribe(&key, &from);
                self.output_events.push_back(RemoteStorageAction(LocalEvent::UnsubAck(req_id, key, success), RouteRule::ToNode(from)));
            }
            RemoteEvent::OnKeySetAck(req_id) => {
                log::debug!("[RemoteStorage] receive on_key_set_ack event from {}, req_id {}", from, req_id);
                self.event_acks.on_ack(req_id);
            }
            RemoteEvent::OnKeyDelAck(req_id) => {
                log::debug!("[RemoteStorage] receive on_key_del_ack event from {}, req_id {}", from, req_id);
                self.event_acks.on_ack(req_id);
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<RemoteStorageAction> {
        //first pop from output_events, if not exits then pop from event_acks
        if let Some(e) = self.output_events.pop_front() {
            log::debug!("[RemoteStorage] pop action from output_events: {:?}", e);
            Some(e)
        } else {
            if let Some(event) = self.storage.poll() {
                let req_id = self.req_id_seed;
                self.req_id_seed += 1;
                let event = match event {
                    OutputEvent::NotifySet(key, value, version, handler) => RemoteStorageAction(LocalEvent::OnKeySet(req_id, key, value, version), RouteRule::ToNode(handler)),
                    OutputEvent::NotifyDel(key, _value, version, handler) => RemoteStorageAction(LocalEvent::OnKeyDel(req_id, key, version), RouteRule::ToNode(handler)),
                };
                log::debug!("[RemoteStorage] pop action from event_acks: {:?}, req_id {}", event, req_id);
                self.event_acks.add_event(req_id, event, RETRY_COUNT);
            }
            self.event_acks.pop_action()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::RemoteStorageAction;
    use crate::msg::{LocalEvent, RemoteEvent};
    use bluesea_router::RouteRule;
    use utils::MockTimer;

    #[test]
    fn receive_set_dersiered_send_ack() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 0, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_set_ex() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 0, Some(1000)));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        assert_eq!(remote_storage.storage.len(), 1);
        timer.fake(500);
        remote_storage.tick();
        assert_eq!(remote_storage.storage.len(), 1);

        timer.fake(1000);
        remote_storage.tick();
        assert_eq!(remote_storage.storage.len(), 0);
    }

    #[test]
    fn receive_set_with_wrong_version() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 10, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 10, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        // receive a older version will be rejected
        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 5, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 5, false), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_dersiered_send_ack() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 0, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1000, RemoteEvent::Del(2, 1, 0));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::DelAck(2, 1, Some(0)), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_older_version() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 10, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 10, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1000, RemoteEvent::Del(2, 1, 5));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::DelAck(2, 1, None), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_newer_version() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 0, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1000, RemoteEvent::Del(2, 1, 100));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::DelAck(2, 1, Some(0)), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_get_dersiered_send_ack() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Set(1, 1, vec![1], 10, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(1, 1, 10, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1001, RemoteEvent::Get(2, 1));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(LocalEvent::GetAck(2, 1, Some((vec![1], 10))), RouteRule::ToNode(1001)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1001, RemoteEvent::Get(3, 2));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::GetAck(3, 2, None), RouteRule::ToNode(1001))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_sub_dersiered_send_ack() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1001, RemoteEvent::Set(2, 1, vec![1], 100, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(2, 1, 100, true), RouteRule::ToNode(1001))));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(LocalEvent::OnKeySet(0, 1, vec![1], 100), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_unsub_dersiered_send_ack() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1000, RemoteEvent::Unsub(2, 1));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::UnsubAck(2, 1, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1001, RemoteEvent::Set(3, 1, vec![1], 100, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(3, 1, 100, true), RouteRule::ToNode(1001))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_unsub_wrong_key() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Unsub(2, 1));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::UnsubAck(2, 1, false), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn key_changed_event_with_ack() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1001, RemoteEvent::Set(2, 1, vec![1], 100, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(2, 1, 100, true), RouteRule::ToNode(1001))));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(LocalEvent::OnKeySet(0, 1, vec![1], 100), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1000, RemoteEvent::OnKeySetAck(0));
        remote_storage.tick();
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn key_changed_event_without_ack_should_resend() {
        let timer = Arc::new(MockTimer::default());
        let mut remote_storage = super::RemoteStorage::new(timer.clone());

        remote_storage.on_event(1000, RemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(1001, RemoteEvent::Set(2, 1, vec![1], 100, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(LocalEvent::SetAck(2, 1, 100, true), RouteRule::ToNode(1001))));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(LocalEvent::OnKeySet(0, 1, vec![1], 100), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.tick();
        // need resend each tick
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(LocalEvent::OnKeySet(0, 1, vec![1], 100), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }
}

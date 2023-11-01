use bluesea_identity::NodeId;
use bluesea_router::RouteRule;
use std::collections::VecDeque;
/// This remote storage is a simple key value storage, it will store all key value in memory, and send event to other node when key value changed
/// Each event is attached with a req_id and wait for ack, if ack not receive, it will resend the event each tick util ack received or tick_count is 0

use crate::storage::simple::{OutputEvent, SimpleKeyValue};
use crate::{
    msg::{SimpleLocalEvent, SimpleRemoteEvent},
    KeyId, ValueType,
};

use super::event_acks::EventAckManager;

const RETRY_COUNT: u8 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteStorageAction(pub(crate) SimpleLocalEvent, pub(crate) RouteRule);

pub struct SimpleRemoteStorage {
    req_id_seed: u64,
    storage: SimpleKeyValue<KeyId, ValueType, NodeId, NodeId>,
    event_acks: EventAckManager<RemoteStorageAction>,
    output_events: VecDeque<RemoteStorageAction>,
}

impl SimpleRemoteStorage {
    pub fn new() -> Self {
        Self {
            req_id_seed: 0,
            storage: SimpleKeyValue::new(),
            event_acks: EventAckManager::new(),
            output_events: VecDeque::new(),
        }
    }

    pub fn tick(&mut self, now_ms: u64) {
        self.storage.tick(now_ms);
        self.event_acks.tick();
    }

    pub fn on_event(&mut self, now_ms: u64, from: NodeId, event: SimpleRemoteEvent) {
        match event {
            SimpleRemoteEvent::Set(req_id, key, value, version, ex) => {
                log::debug!("[SimpleRemote] receive set event from {} key {} value {:?} version {} ex {:?}", from, key, value, version, ex);
                let setted = self.storage.set(now_ms, key, value, version, from, ex);
                self.output_events
                    .push_back(RemoteStorageAction(SimpleLocalEvent::SetAck(req_id, key, version, setted), RouteRule::ToNode(from)));
            }
            SimpleRemoteEvent::Get(req_id, key) => {
                if let Some((value, version, source)) = self.storage.get(&key) {
                    log::debug!("[SimpleRemote] receive get event from {} key {} value {:?} version {}", from, key, value, version);
                    self.output_events.push_back(RemoteStorageAction(
                        SimpleLocalEvent::GetAck(req_id, key, Some((value.clone(), version, source))),
                        RouteRule::ToNode(from),
                    ));
                } else {
                    log::debug!("[SimpleRemote] receive get event from {} key {} value None", from, key);
                    self.output_events.push_back(RemoteStorageAction(SimpleLocalEvent::GetAck(req_id, key, None), RouteRule::ToNode(from)));
                }
            }
            SimpleRemoteEvent::Del(req_id, key, req_version) => {
                log::debug!("[SimpleRemote] receive del event from {} key {} version {:?}", from, key, req_version);
                let version = self.storage.del(&key, req_version).map(|(_, version, _)| version);
                self.output_events
                    .push_back(RemoteStorageAction(SimpleLocalEvent::DelAck(req_id, key, version), RouteRule::ToNode(from)));
            }
            SimpleRemoteEvent::Sub(req_id, key, ex) => {
                log::debug!("[SimpleRemote] receive sub event from {} key {} ex {:?}", from, key, ex);
                self.storage.subscribe(now_ms, &key, from, ex);
                self.output_events.push_back(RemoteStorageAction(SimpleLocalEvent::SubAck(req_id, key), RouteRule::ToNode(from)));
            }
            SimpleRemoteEvent::Unsub(req_id, key) => {
                log::debug!("[SimpleRemote] receive unsub event from {} key {}", from, key);
                let success = self.storage.unsubscribe(&key, &from);
                self.output_events
                    .push_back(RemoteStorageAction(SimpleLocalEvent::UnsubAck(req_id, key, success), RouteRule::ToNode(from)));
            }
            SimpleRemoteEvent::OnKeySetAck(req_id) => {
                log::debug!("[SimpleRemote] receive on_key_set_ack event from {}, req_id {}", from, req_id);
                self.event_acks.on_ack(req_id);
            }
            SimpleRemoteEvent::OnKeyDelAck(req_id) => {
                log::debug!("[SimpleRemote] receive on_key_del_ack event from {}, req_id {}", from, req_id);
                self.event_acks.on_ack(req_id);
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<RemoteStorageAction> {
        //first pop from output_events, if not exits then pop from event_acks
        if let Some(e) = self.output_events.pop_front() {
            log::debug!("[SimpleRemote] pop action from output_events: {:?}", e);
            Some(e)
        } else {
            if let Some(event) = self.storage.poll() {
                let req_id = self.req_id_seed;
                self.req_id_seed += 1;
                let event = match event {
                    OutputEvent::NotifySet(key, value, version, source, handler) => RemoteStorageAction(SimpleLocalEvent::OnKeySet(req_id, key, value, version, source), RouteRule::ToNode(handler)),
                    OutputEvent::NotifyDel(key, _value, version, source, handler) => RemoteStorageAction(SimpleLocalEvent::OnKeyDel(req_id, key, version, source), RouteRule::ToNode(handler)),
                };
                log::debug!("[SimpleRemote] pop action from event_acks: {:?}, req_id {}", event, req_id);
                self.event_acks.add_event(req_id, event, RETRY_COUNT);
            }
            self.event_acks.pop_action()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteStorageAction;
    use crate::msg::{SimpleLocalEvent, SimpleRemoteEvent};
    use bluesea_router::RouteRule;

    #[test]
    fn receive_set_dersiered_send_ack() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 0, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_set_ex() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 0, Some(1000)));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        assert_eq!(remote_storage.storage.len(), 1);
        remote_storage.tick(500);
        assert_eq!(remote_storage.storage.len(), 1);

        remote_storage.tick(1000);
        assert_eq!(remote_storage.storage.len(), 0);
    }

    #[test]
    fn receive_set_with_wrong_version() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 10, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 10, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        // receive a older version will be rejected
        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 5, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 5, false), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_dersiered_send_ack() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 0, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Del(2, 1, 0));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::DelAck(2, 1, Some(0)), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_older_version() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 10, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 10, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Del(2, 1, 5));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::DelAck(2, 1, None), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_newer_version() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 0, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 0, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Del(2, 1, 100));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::DelAck(2, 1, Some(0)), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_get_dersiered_send_ack() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Set(1, 1, vec![1], 10, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(1, 1, 10, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, SimpleRemoteEvent::Get(2, 1));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::GetAck(2, 1, Some((vec![1], 10, 1000))), RouteRule::ToNode(1001)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, SimpleRemoteEvent::Get(3, 2));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::GetAck(3, 2, None), RouteRule::ToNode(1001))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_sub_dersiered_send_ack() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, SimpleRemoteEvent::Set(2, 1, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(2, 1, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::OnKeySet(0, 1, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_sub_after_set_dersiered_send_ack() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1001, SimpleRemoteEvent::Set(2, 1, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(2, 1, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::OnKeySet(0, 1, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_unsub_dersiered_send_ack() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Unsub(2, 1));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::UnsubAck(2, 1, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, SimpleRemoteEvent::Set(3, 1, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(3, 1, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_unsub_wrong_key() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Unsub(2, 1));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::UnsubAck(2, 1, false), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn key_changed_event_with_ack() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, SimpleRemoteEvent::Set(2, 1, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(2, 1, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::OnKeySet(0, 1, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::OnKeySetAck(0));
        remote_storage.tick(0);
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn key_changed_event_without_ack_should_resend() {
        let mut remote_storage = super::SimpleRemoteStorage::new();

        remote_storage.on_event(0, 1000, SimpleRemoteEvent::Sub(1, 1, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(SimpleLocalEvent::SubAck(1, 1), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, SimpleRemoteEvent::Set(2, 1, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::SetAck(2, 1, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::OnKeySet(0, 1, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.tick(0);
        // need resend each tick
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(SimpleLocalEvent::OnKeySet(0, 1, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }
}

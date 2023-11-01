use bluesea_identity::NodeId;
use bluesea_router::RouteRule;
use std::collections::VecDeque;
/// This remote storage is a hashmap key value storage, it will store all key value in memory, and send event to other node when key value changed
/// Each event is attached with a req_id and wait for ack, if ack not receive, it will resend the event each tick util ack received or tick_count is 0

use crate::storage::hashmap::{HashmapKeyValue, OutputEvent};
use crate::SubKeyId;
use crate::{
    msg::{HashmapLocalEvent, HashmapRemoteEvent},
    KeyId, ValueType,
};

use super::event_acks::EventAckManager;

const RETRY_COUNT: u8 = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteStorageAction(pub(crate) HashmapLocalEvent, pub(crate) RouteRule);

pub struct HashmapRemoteStorage {
    node_id: NodeId,
    req_id_seed: u64,
    storage: HashmapKeyValue<KeyId, SubKeyId, ValueType, NodeId, NodeId>,
    event_acks: EventAckManager<RemoteStorageAction>,
    output_events: VecDeque<RemoteStorageAction>,
}

impl HashmapRemoteStorage {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            req_id_seed: 0,
            storage: HashmapKeyValue::new(),
            event_acks: EventAckManager::new(),
            output_events: VecDeque::new(),
        }
    }

    pub fn tick(&mut self, now_ms: u64) {
        self.storage.tick(now_ms);
        self.event_acks.tick();
    }

    pub fn on_event(&mut self, now_ms: u64, from: NodeId, event: HashmapRemoteEvent) {
        match event {
            HashmapRemoteEvent::Set(req_id, key, sub_key, value, version, ex) => {
                log::debug!(
                    "[HashmapRemote {}] receive set event from {} key {} sub_key {} value {:?} version {} ex {:?}",
                    self.node_id,
                    from,
                    key,
                    sub_key,
                    value,
                    version,
                    ex
                );
                let setted = self.storage.set(now_ms, key, sub_key, value, version, from, ex);
                self.output_events
                    .push_back(RemoteStorageAction(HashmapLocalEvent::SetAck(req_id, key, sub_key, version, setted), RouteRule::ToNode(from)));
            }
            HashmapRemoteEvent::Get(req_id, key) => {
                if let Some(sub_keys) = self.storage.get(&key) {
                    log::debug!("[HashmapRemote {}] receive get event from {} has {} keys", self.node_id, from, sub_keys.len());
                    let sub_keys_clone = sub_keys
                        .into_iter()
                        .map(|(sub_key, value, version, source)| (sub_key, value.clone(), version, source))
                        .collect::<Vec<_>>();
                    self.output_events
                        .push_back(RemoteStorageAction(HashmapLocalEvent::GetAck(req_id, key, Some(sub_keys_clone)), RouteRule::ToNode(from)));
                } else {
                    log::debug!("[HashmapRemote {}] receive get event from {} key {} value None", self.node_id, from, key);
                    self.output_events.push_back(RemoteStorageAction(HashmapLocalEvent::GetAck(req_id, key, None), RouteRule::ToNode(from)));
                }
            }
            HashmapRemoteEvent::Del(req_id, key, sub_key, req_version) => {
                log::debug!(
                    "[HashmapRemote {}] receive del event from {} key {} sub_key {} version {:?}",
                    self.node_id,
                    from,
                    key,
                    sub_key,
                    req_version
                );
                let version = self.storage.del(&key, &sub_key, req_version).map(|(_, version, _)| version);
                self.output_events
                    .push_back(RemoteStorageAction(HashmapLocalEvent::DelAck(req_id, key, sub_key, version), RouteRule::ToNode(from)));
            }
            HashmapRemoteEvent::Sub(req_id, key, ex) => {
                log::debug!("[HashmapRemote {}] receive sub event from {} key {} ex {:?}", self.node_id, from, key, ex);
                self.storage.subscribe(now_ms, &key, from, ex);
                self.output_events.push_back(RemoteStorageAction(HashmapLocalEvent::SubAck(req_id, key), RouteRule::ToNode(from)));
            }
            HashmapRemoteEvent::Unsub(req_id, key) => {
                log::debug!("[HashmapRemote {}] receive unsub event from {} key {}", self.node_id, from, key);
                let success = self.storage.unsubscribe(&key, &from);
                self.output_events
                    .push_back(RemoteStorageAction(HashmapLocalEvent::UnsubAck(req_id, key, success), RouteRule::ToNode(from)));
            }
            HashmapRemoteEvent::OnKeySetAck(req_id) => {
                log::debug!("[HashmapRemote {}] receive on_key_set_ack event from {}, self.node_id, req_id {}", self.node_id, from, req_id);
                self.event_acks.on_ack(req_id);
            }
            HashmapRemoteEvent::OnKeyDelAck(req_id) => {
                log::debug!("[HashmapRemote {}] receive on_key_del_ack event from {}, self.node_id, req_id {}", self.node_id, from, req_id);
                self.event_acks.on_ack(req_id);
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<RemoteStorageAction> {
        //first pop from output_events, if not exits then pop from event_acks
        if let Some(e) = self.output_events.pop_front() {
            log::debug!("[HashmapRemote {}] pop action from output_events: {:?}", self.node_id, e);
            Some(e)
        } else {
            if let Some(event) = self.storage.poll() {
                let req_id = self.req_id_seed;
                self.req_id_seed += 1;
                let event = match event {
                    OutputEvent::NotifySet(key, sub_key, value, version, source, handler) => {
                        RemoteStorageAction(HashmapLocalEvent::OnKeySet(req_id, key, sub_key, value, version, source), RouteRule::ToNode(handler))
                    }
                    OutputEvent::NotifyDel(key, sub_key, _value, version, source, handler) => {
                        RemoteStorageAction(HashmapLocalEvent::OnKeyDel(req_id, key, sub_key, version, source), RouteRule::ToNode(handler))
                    }
                };
                log::debug!("[HashmapRemote {}] pop action from event_acks: {:?}, req_id {}", self.node_id, event, req_id);
                self.event_acks.add_event(req_id, event, RETRY_COUNT);
            }
            self.event_acks.pop_action()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteStorageAction;
    use crate::msg::{HashmapLocalEvent, HashmapRemoteEvent};
    use bluesea_router::RouteRule;

    #[test]
    fn receive_set_dersiered_send_ack() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 0, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 0, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_set_ex() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 0, Some(1000)));
        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(2, 2, 4, vec![1], 0, Some(2000)));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 0, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(2, 2, 4, 0, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        assert_eq!(remote_storage.storage.len(), 1);
        remote_storage.tick(500);
        assert_eq!(remote_storage.storage.len(), 1);

        remote_storage.tick(1000);
        assert_eq!(remote_storage.storage.len(), 1);

        remote_storage.tick(2000);
        assert_eq!(remote_storage.storage.len(), 0);
    }

    #[test]
    fn receive_set_with_wrong_version() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 10, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 10, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        // receive a older version will be rejected
        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 5, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 5, false), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_dersiered_send_ack() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 0, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 0, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Del(2, 2, 3, 0));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::DelAck(2, 2, 3, Some(0)), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_older_version() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 10, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 10, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Del(2, 2, 3, 5));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::DelAck(2, 2, 3, None), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_del_newer_version() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 0, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 0, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Del(2, 2, 3, 100));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::DelAck(2, 2, 3, Some(0)), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_get_dersiered_send_ack() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Set(1, 2, 3, vec![1], 10, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(1, 2, 3, 10, true), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, HashmapRemoteEvent::Get(2, 2));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::GetAck(2, 2, Some(vec![(3, vec![1], 10, 1000)])), RouteRule::ToNode(1001)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, HashmapRemoteEvent::Get(3, 5));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(HashmapLocalEvent::GetAck(3, 5, None), RouteRule::ToNode(1001))));
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_sub_dersiered_send_ack() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Sub(1, 2, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(HashmapLocalEvent::SubAck(1, 2), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, HashmapRemoteEvent::Set(2, 2, 3, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(2, 2, 3, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::OnKeySet(0, 2, 3, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_sub_after_set_dersiered_send_ack() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1001, HashmapRemoteEvent::Set(2, 2, 3, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(2, 2, 3, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Sub(1, 2, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(HashmapLocalEvent::SubAck(1, 2), RouteRule::ToNode(1000))));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::OnKeySet(0, 2, 3, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_unsub_dersiered_send_ack() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Sub(1, 2, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(HashmapLocalEvent::SubAck(1, 2), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Unsub(2, 2));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(HashmapLocalEvent::UnsubAck(2, 2, true), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, HashmapRemoteEvent::Set(3, 2, 3, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(3, 2, 3, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn receive_unsub_wrong_key() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Unsub(2, 1));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::UnsubAck(2, 1, false), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn key_changed_event_with_ack() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Sub(1, 2, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(HashmapLocalEvent::SubAck(1, 2), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, HashmapRemoteEvent::Set(2, 2, 3, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(2, 2, 3, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::OnKeySet(0, 2, 3, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::OnKeySetAck(0));
        remote_storage.tick(0);
        assert_eq!(remote_storage.pop_action(), None);
    }

    #[test]
    fn key_changed_event_without_ack_should_resend() {
        let mut remote_storage = super::HashmapRemoteStorage::new(0);

        remote_storage.on_event(0, 1000, HashmapRemoteEvent::Sub(1, 2, None));
        assert_eq!(remote_storage.pop_action(), Some(RemoteStorageAction(HashmapLocalEvent::SubAck(1, 2), RouteRule::ToNode(1000))));
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.on_event(0, 1001, HashmapRemoteEvent::Set(2, 2, 3, vec![1], 100, None));
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::SetAck(2, 2, 3, 100, true), RouteRule::ToNode(1001)))
        );
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::OnKeySet(0, 2, 3, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);

        remote_storage.tick(0);
        // need resend each tick
        assert_eq!(
            remote_storage.pop_action(),
            Some(RemoteStorageAction(HashmapLocalEvent::OnKeySet(0, 2, 3, vec![1], 100, 1001), RouteRule::ToNode(1000)))
        );
        assert_eq!(remote_storage.pop_action(), None);
    }
}

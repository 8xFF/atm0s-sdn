use std::sync::Arc;
use bluesea_identity::NodeId;
use bluesea_router::RouteRule;
use utils::Timer;

use crate::storage::simple::{SimpleKeyValue, OutputEvent};
use crate::{KeyId, ValueType, msg::{RemoteEvent, LocalEvent}};

use super::event_acks::EventAckManager;

#[derive(Clone)]
pub struct RemoveStorageAction(pub(crate) LocalEvent, pub(crate) RouteRule);

pub struct RemoteStorage {
    req_id_seed: u64,
    storage: SimpleKeyValue<KeyId, ValueType, NodeId>,
    event_acks: EventAckManager<RemoveStorageAction>,
    output_events: Vec<RemoveStorageAction>,
}

impl RemoteStorage {
    pub fn new(timer: Arc<dyn Timer>) -> Self {
        Self {
            req_id_seed: 0,
            storage: SimpleKeyValue::new(timer),
            event_acks: EventAckManager::new(),
            output_events: Vec::new(),
        }
    }

    pub fn tick(&mut self) {
        self.storage.tick();
        self.event_acks.tick();
    }

    pub fn on_event(&mut self, from: NodeId, event: RemoteEvent) {
        match event {
            RemoteEvent::Set(req_id, key, value, version, ex) => {
                if self.storage.set(key, value, version, ex) {
                    self.output_events.push(RemoveStorageAction(LocalEvent::SetAck(req_id, key, version), RouteRule::ToNode(from)));
                }
            },
            RemoteEvent::Get(req_id, key) => {
                if let Some((value, version)) = self.storage.get(&key) {
                    self.output_events.push(RemoveStorageAction(LocalEvent::GetAck(req_id, key, Some((value.clone(), version))), RouteRule::ToNode(from)));
                } else {
                    self.output_events.push(RemoveStorageAction(LocalEvent::GetAck(req_id, key, None), RouteRule::ToNode(from)));
                }
            },
            RemoteEvent::Del(req_id, key, req_version) => {
                let version = self.storage.del(&key, req_version).map(|(_, version)| version);
                self.output_events.push(RemoveStorageAction(LocalEvent::DelAck(req_id, key, version), RouteRule::ToNode(from)));
            },
            RemoteEvent::Sub(req_id, key, ex) => {
                self.storage.subscribe(&key, from, ex);
                self.output_events.push(RemoveStorageAction(LocalEvent::SubAck(req_id, key), RouteRule::ToNode(from)));
            },
            RemoteEvent::Unsub(req_id, key) => {
                self.storage.unsubscribe(&key, &from);
                self.output_events.push(RemoveStorageAction(LocalEvent::UnsubAck(req_id, key), RouteRule::ToNode(from)));
            },
            RemoteEvent::OnKeySetAck(req_id) => {
                self.event_acks.on_ack(req_id);
            },
            RemoteEvent::OnKeyDelAck(req_id) => {
                self.event_acks.on_ack(req_id);
            },
        }
    }

    pub fn pop_action(&mut self) -> Option<RemoveStorageAction> {
        //first pop from output_events, if not exits then pop from event_acks
        if let Some(e) = self.output_events.pop() {
            Some(e)
        } else {
            if let Some(event) = self.storage.poll() {
                let req_id = self.req_id_seed;
                self.req_id_seed += 1;
                let event = match event {
                    OutputEvent::NotifySet(key, value, version, handler) => {
                        RemoveStorageAction(LocalEvent::OnKeySet(req_id, key, value, version), RouteRule::ToNode(handler))
                    },
                    OutputEvent::NotifyDel(key, _value, version, handler) => {
                        RemoveStorageAction(LocalEvent::OnKeyDel(req_id, key, version), RouteRule::ToNode(handler))
                    },
                };
                self.event_acks.add_event(req_id, event, 5);
            }
            self.event_acks.pop_action()
        }
    }
}
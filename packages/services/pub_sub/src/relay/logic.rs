use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    sync::Arc,
};

use bluesea_identity::{ConnId, NodeId};
use utils::Timer;

use crate::{msg::PubsubRemoteEvent, PUBSUB_CHANNEL_RESYNC_MS};

use super::{ChannelIdentify, LocalSubId};

struct AckedInfo {
    from_node: NodeId,
    from_conn: ConnId,
    at_ms: u64,
}

struct ChannelContainer {
    source: NodeId,
    acked: Option<AckedInfo>,
    remote_subscribers: Vec<ConnId>,
    local_subscribers: Vec<LocalSubId>,
}

pub struct PubsubRelayLogic {
    timer: Arc<dyn Timer>,
    node_id: NodeId,
    channels: HashMap<ChannelIdentify, ChannelContainer>,
    output_events: VecDeque<(NodeId, Option<ConnId>, PubsubRemoteEvent)>,
}

impl PubsubRelayLogic {
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>) -> Self {
        Self {
            timer,
            node_id,
            channels: Default::default(),
            output_events: Default::default(),
        }
    }

    /// In case of source not in current node:
    ///
    /// - we need to send sub event to next node if acked is None and subscribes is not empty
    /// - or we need to send unsub event to next node if acked is Some and subscribes is empty
    /// - we need resend each PUBSUB_CHANNEL_RESYNC_MS
    /// - we need to timeout key if no acked in PUBSUB_CHANNEL_TIMEOUT_MS
    pub fn tick(&mut self) {
        for (channel, slot) in self.channels.iter() {
            if channel.source() == self.node_id {
                continue;
            }
            if slot.remote_subscribers.len() + slot.local_subscribers.len() > 0 {
                if let Some(acked) = &slot.acked {
                    let now_ms = self.timer.now_ms();
                    if now_ms - acked.at_ms >= PUBSUB_CHANNEL_RESYNC_MS {
                        self.output_events.push_back((channel.source(), None, PubsubRemoteEvent::Sub(*channel)));
                    }
                } else {
                    self.output_events.push_back((channel.source(), None, PubsubRemoteEvent::Sub(*channel)));
                }
            } else if slot.acked.is_some() && slot.remote_subscribers.len() == 0 && slot.local_subscribers.len() == 0 {
                if let Some(info) = &slot.acked {
                    self.output_events.push_back((info.from_node, Some(info.from_conn), PubsubRemoteEvent::Unsub(*channel)));
                }
            }
        }
    }

    /// This node subscribe that channel,
    /// then we must to check if that channel already exist or not,
    /// if not, we must to send a sub event to the source node
    pub fn on_local_sub(&mut self, channel: ChannelIdentify, handler: LocalSubId) {
        let maybe_sub = match self.channels.entry(channel) {
            Entry::Occupied(mut entry) => {
                let value = entry.get_mut();
                // if from conn is not in value.remotes list => push to list
                // if from conn is in value.remotes list => do nothing
                if !value.local_subscribers.contains(&handler) {
                    value.local_subscribers.push(handler);
                }
                false
            }
            Entry::Vacant(entry) => {
                entry.insert(ChannelContainer {
                    source: channel.source(),
                    acked: None,
                    remote_subscribers: vec![],
                    local_subscribers: vec![handler],
                });
                true
            }
        };

        if maybe_sub && channel.source() != self.node_id {
            self.output_events.push_back((channel.source(), None, PubsubRemoteEvent::Sub(channel)));
        }
    }

    /// This node unsubscribe that channle,
    /// then we must to check if no one subscribe that channel,
    /// if no one, we must to send a unsub event to the source node
    pub fn on_local_unsub(&mut self, channel: ChannelIdentify, handler: LocalSubId) {
        if let Some(slot) = self.channels.get_mut(&channel) {
            if let Some(index) = slot.local_subscribers.iter().position(|&x| x == handler) {
                slot.remote_subscribers.swap_remove(index);
            }

            if slot.remote_subscribers.len() == 0 && slot.local_subscribers.len() == 0 {
                if let Some(info) = &slot.acked {
                    self.output_events.push_back((info.from_node, Some(info.from_conn), PubsubRemoteEvent::Unsub(channel)));
                }
            }
        }
    }

    pub fn on_event(&mut self, from: NodeId, conn: ConnId, event: PubsubRemoteEvent) {
        match event {
            PubsubRemoteEvent::Sub(id) => {
                let maybe_sub = match self.channels.entry(id) {
                    Entry::Occupied(mut entry) => {
                        let value = entry.get_mut();
                        // if from conn is not in value.remotes list => push to list
                        // if from conn is in value.remotes list => do nothing
                        if !value.remote_subscribers.contains(&conn) {
                            value.remote_subscribers.push(conn);
                            log::info!("[PubsubRelayLogic] sub {} event from {} pushed to list", id, from);
                            self.output_events.push_back((from, Some(conn), PubsubRemoteEvent::SubAck(id, true)));
                        } else {
                            log::info!("[PubsubRelayLogic] sub {} event from {} allready added", id, from);
                            self.output_events.push_back((from, Some(conn), PubsubRemoteEvent::SubAck(id, false)));
                        }
                        false
                    }
                    Entry::Vacant(entry) => {
                        log::info!("[PubsubRelayLogic] sub {} event from {} pushed to list, new relay", id, from);
                        entry.insert(ChannelContainer {
                            source: id.source(),
                            acked: None,
                            remote_subscribers: vec![conn],
                            local_subscribers: vec![],
                        });
                        self.output_events.push_back((from, Some(conn), PubsubRemoteEvent::SubAck(id, true)));
                        true
                    }
                };

                if maybe_sub && id.source() != self.node_id {
                    log::info!("[PubsubRelayLogic] sub {} event from {} pushed to list, new relay => send sub to source {}", id, from, id.source());
                    self.output_events.push_back((id.source(), None, PubsubRemoteEvent::Sub(id)));
                }
            }
            PubsubRemoteEvent::Unsub(id) => {
                if let Some(slot) = self.channels.get_mut(&id) {
                    if let Some(index) = slot.remote_subscribers.iter().position(|&x| x == conn) {
                        slot.remote_subscribers.swap_remove(index);
                        log::info!("[PubsubRelayLogic] unsub {} event from {} removed from list", id, from);
                        self.output_events.push_back((from, Some(conn), PubsubRemoteEvent::UnsubAck(id, true)));
                    } else {
                        log::info!("[PubsubRelayLogic] unsub {} event from {} allready removed from list", id, from);
                        self.output_events.push_back((from, Some(conn), PubsubRemoteEvent::UnsubAck(id, false)));
                    }

                    if slot.remote_subscribers.len() == 0 && slot.local_subscribers.len() == 0 {
                        if slot.source != self.node_id {
                            if let Some(info) = &slot.acked {
                                log::info!("[PubsubRelayLogic] unsub {} event from {} list empty => send unsub to next node {}", id, from, info.from_node);
                                self.output_events.push_back((info.from_node, Some(info.from_conn), PubsubRemoteEvent::Unsub(id)));
                            } else {
                                self.channels.remove(&id);
                                log::warn!("[PubsubRelayLogic] unsub {} event from {} list empty => but no next node", id, from);
                            }
                        } else {
                            self.channels.remove(&id);
                            log::info!("[PubsubRelayLogic] unsub {} event from {} list empty in source node => removed", id, from);
                        }
                    }
                } else {
                    log::warn!("[PubsubRelayLogic] unsub {} event from {} but no channel found", id, from);
                }
            }
            PubsubRemoteEvent::SubAck(id, _added) => {
                if let Some(slot) = self.channels.get_mut(&id) {
                    log::info!("[PubsubRelayLogic] sub_ack {} event from {}", id, from);
                    slot.acked = Some(AckedInfo {
                        from_node: from,
                        from_conn: conn,
                        at_ms: self.timer.now_ms(),
                    });
                } else {
                    log::warn!("[PubsubRelayLogic] sub_ack {} event from {} but channel not found", id, from);
                }
            }
            PubsubRemoteEvent::UnsubAck(id, _removed) => {
                if self.channels.remove(&id).is_some() {
                    log::info!("[PubsubRelayLogic] unsub_ack {} event from {}", id, from);
                } else {
                    log::warn!("[PubsubRelayLogic] unsub_ack {} event from {} but channel not found", id, from);
                }
            }
        }
    }

    pub fn relay(&self, channel: ChannelIdentify) -> Option<(&[ConnId], &[LocalSubId])> {
        let slot = self.channels.get(&channel)?;
        Some((&slot.remote_subscribers, &slot.local_subscribers))
    }

    pub fn pop_action(&mut self) -> Option<(NodeId, Option<ConnId>, PubsubRemoteEvent)> {
        self.output_events.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bluesea_identity::{ConnId, NodeId};
    use utils::MockTimer;

    use crate::{msg::PubsubRemoteEvent, relay::ChannelIdentify, PUBSUB_CHANNEL_RESYNC_MS};

    use super::PubsubRelayLogic;

    enum Event {
        FakeTimer(u64),
        Tick,
        In(NodeId, ConnId, PubsubRemoteEvent),
        OutNone,
        Out(NodeId, Option<ConnId>, PubsubRemoteEvent),
        Validate(Box<dyn FnOnce(&PubsubRelayLogic) -> bool>),
    }

    fn test(node_id: NodeId, events: Vec<Event>) {
        let timer = Arc::new(MockTimer::default());
        let mut logic = PubsubRelayLogic::new(node_id, timer.clone());

        for event in events {
            match event {
                Event::FakeTimer(ts) => timer.fake(ts),
                Event::Tick => logic.tick(),
                Event::In(from, conn, event) => logic.on_event(from, conn, event),
                Event::OutNone => assert_eq!(logic.pop_action(), None),
                Event::Out(from, conn, event) => assert_eq!(logic.pop_action(), Some((from, conn, event))),
                Event::Validate(validator) => assert_eq!(validator(&logic), true),
            }
        }
    }

    /// This test case ensure sub event to source node only generate sub ack
    #[test]
    fn in_source_remote_simple() {
        let node_id = 0;

        let channel = ChannelIdentify::new(111, node_id);

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        test(
            node_id,
            vec![
                Event::Tick,
                Event::OutNone,
                Event::In(remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::OutNone,
                Event::Tick,
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::In(remote_node_id, remote_conn_id, PubsubRemoteEvent::Unsub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }

    /// This test case ensure sub event is relayed to source node
    #[test]
    fn in_relay_remote_simple() {
        let node_id = 1000;

        let channel = ChannelIdentify::new(111, 0);

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        let next_node_id = 2;
        let next_conn_id = ConnId::from_in(10, 3);

        test(
            node_id,
            vec![
                Event::Tick,
                Event::OutNone,
                Event::In(remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutNone,
                Event::In(next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick,
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::In(remote_node_id, remote_conn_id, PubsubRemoteEvent::Unsub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::Out(next_node_id, Some(next_conn_id), PubsubRemoteEvent::Unsub(channel)),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::In(next_node_id, next_conn_id, PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }

    /// This test ensure sub resend util acked
    #[test]
    fn in_relay_resend_sub() {
        let node_id = 1000;

        let channel = ChannelIdentify::new(111, 0);

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        let next_node_id = 2;
        let next_conn_id = ConnId::from_in(10, 3);

        test(
            node_id,
            vec![
                Event::Tick,
                Event::OutNone,
                Event::In(remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutNone,
                Event::Tick,
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutNone,
                Event::In(next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick,
                Event::OutNone,
            ],
        );
    }

    #[test]
    fn in_relay_resend_sync() {
        let node_id = 1000;

        let channel = ChannelIdentify::new(111, 0);

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        let next_node_id = 2;
        let next_conn_id = ConnId::from_in(10, 3);

        test(
            node_id,
            vec![
                Event::Tick,
                Event::OutNone,
                Event::In(remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutNone,
                Event::In(next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick,
                Event::OutNone,
                Event::FakeTimer(PUBSUB_CHANNEL_RESYNC_MS),
                Event::Tick,
                Event::Out(next_node_id, Some(next_conn_id), PubsubRemoteEvent::Sub(channel)),
                Event::OutNone,
            ],
        );
    }
}

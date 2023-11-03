use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    sync::Arc,
};

use bluesea_identity::{ConnId, NodeId};
use utils::awaker::{Awaker, MockAwaker};

use crate::{msg::PubsubRemoteEvent, PUBSUB_CHANNEL_RESYNC_MS, PUBSUB_CHANNEL_TIMEOUT_MS};

use super::{
    feedback::{ChannelFeedbackProcessor, Feedback, FeedbackConsumerId},
    ChannelIdentify, LocalSubId,
};

struct AckedInfo {
    from_node: NodeId,
    from_conn: ConnId,
    at_ms: u64,
}

struct ChannelContainer {
    source: NodeId,
    acked: Option<AckedInfo>,
    remote_subscribers: Vec<ConnId>,
    remote_subscribers_ts: HashMap<ConnId, u64>,
    local_subscribers: Vec<LocalSubId>,
    feedback_processor: ChannelFeedbackProcessor,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PubsubRelayLogicOutput {
    Event(PubsubRemoteEvent),
    Feedback(Feedback),
}

pub struct PubsubRelayLogic {
    awaker: Arc<dyn Awaker>,
    node_id: NodeId,
    channels: HashMap<ChannelIdentify, ChannelContainer>,
    output_events: VecDeque<(NodeId, Option<ConnId>, PubsubRelayLogicOutput)>,
}

impl PubsubRelayLogic {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            awaker: Arc::new(MockAwaker::default()),
            node_id,
            channels: Default::default(),
            output_events: Default::default(),
        }
    }

    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.awaker = awaker;
    }

    /// We need to check each channel for:
    /// - Clear timeout subscribes
    /// - In case of source not in current node:
    ///     - we need to send sub event to next node if acked is None and subscribes is not empty
    ///     - or we need to send unsub event to next node if acked is Some and subscribes is empty
    ///     - we need resend each PUBSUB_CHANNEL_RESYNC_MS
    ///     - we need to timeout key if no acked in PUBSUB_CHANNEL_TIMEOUT_MS
    pub fn tick(&mut self, now_ms: u64) -> Vec<Feedback> {
        let mut local_fbs = vec![];
        let mut need_clear_channels = vec![];
        for (channel, slot) in self.channels.iter_mut() {
            let mut timeout_remotes = vec![];
            for (conn, ts) in slot.remote_subscribers_ts.iter() {
                if now_ms - ts >= PUBSUB_CHANNEL_TIMEOUT_MS {
                    log::info!("[PubsubRelayLogic {}] remote sub {} event from {} timeout", self.node_id, channel, conn);
                    timeout_remotes.push(*conn);
                }
            }
            for conn in timeout_remotes {
                if let Some(index) = slot.remote_subscribers.iter().position(|&x| x == conn) {
                    slot.remote_subscribers.swap_remove(index);
                }
                slot.remote_subscribers_ts.remove(&conn);
            }

            if let Some(mut fbs) = slot.feedback_processor.on_tick(now_ms) {
                if let Some(remote) = &slot.acked {
                    for fb in fbs {
                        self.output_events.push_back((remote.from_node, Some(remote.from_conn), PubsubRelayLogicOutput::Feedback(fb)));
                    }
                } else if self.node_id == channel.source() {
                    local_fbs.append(&mut fbs);
                }
            }

            if channel.source() == self.node_id {
                if slot.remote_subscribers.len() == 0 && slot.local_subscribers.len() == 0 {
                    log::info!("[PubsubRelayLogic {}] channel {} empty in source node => clear", self.node_id, channel);
                    need_clear_channels.push(*channel);
                }
                continue;
            }
            if slot.remote_subscribers.len() + slot.local_subscribers.len() > 0 {
                if let Some(acked) = &slot.acked {
                    let now_ms = now_ms;
                    if now_ms - acked.at_ms >= PUBSUB_CHANNEL_RESYNC_MS {
                        log::info!(
                            "[PubsubRelayLogic {}] resend sub {} event to next node {} in each resync cycle {} ms",
                            self.node_id,
                            channel,
                            acked.from_node,
                            PUBSUB_CHANNEL_RESYNC_MS
                        );
                        //Should be send to correct conn, if that conn not exits => fallback by finding to origin source
                        self.output_events
                            .push_back((channel.source(), Some(acked.from_conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::Sub(*channel))));
                    }
                } else {
                    log::info!(
                        "[PubsubRelayLogic {}] resend sub {} event to next node {} in each because of non-acked channel",
                        self.node_id,
                        channel,
                        channel.source()
                    );
                    self.output_events.push_back((channel.source(), None, PubsubRelayLogicOutput::Event(PubsubRemoteEvent::Sub(*channel))));
                }
            } else if slot.remote_subscribers.len() == 0 && slot.local_subscribers.len() == 0 {
                if let Some(info) = &slot.acked {
                    log::info!("[PubsubRelayLogic {}] resend unsub {} event back next node {} because of empty", self.node_id, channel, info.from_node);
                    self.output_events
                        .push_back((info.from_node, Some(info.from_conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::Unsub(*channel))));
                } else {
                    if self.node_id != channel.source() {
                        log::warn!("[PubsubRelayLogic {}] remove channel {} with no next node info because of empty", self.node_id, channel);
                    } else {
                        log::info!("[PubsubRelayLogic {}] remove channel {} list empty in source node => removed", self.node_id, channel);
                    }
                    need_clear_channels.push(*channel);
                }
            }
        }

        for channel in need_clear_channels {
            self.channels.remove(&channel);
        }

        local_fbs
    }

    /// Process feedback from consumer, return Some(fb) if need to call local publisher feedback
    pub fn on_feedback(&mut self, now_ms: u64, channel: ChannelIdentify, consumer_id: FeedbackConsumerId, fb: Feedback) -> Option<Feedback> {
        if let Some(slot) = self.channels.get_mut(&channel) {
            if let Some(fb) = slot.feedback_processor.on_feedback(now_ms, consumer_id, fb) {
                if let Some(remote) = &slot.acked {
                    self.output_events.push_back((remote.from_node, Some(remote.from_conn), PubsubRelayLogicOutput::Feedback(fb)));
                    self.awaker.notify();
                    None
                } else if self.node_id == channel.source() {
                    Some(fb)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            log::warn!("[PubsubRelayLogic {}] feedback {} event but no channel found", self.node_id, channel);
            None
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
                    log::info!("[PubsubRelayLogic {}] local sub {} event from {} pushed to list", self.node_id, channel, handler);
                } else {
                    log::info!("[PubsubRelayLogic {}] local sub {} event from {} allready in list", self.node_id, channel, handler);
                }
                false
            }
            Entry::Vacant(entry) => {
                log::info!("[PubsubRelayLogic {}] local sub {} event from {} pushed to list, new relay", self.node_id, channel, handler);
                entry.insert(ChannelContainer {
                    source: channel.source(),
                    acked: None,
                    remote_subscribers: vec![],
                    remote_subscribers_ts: HashMap::new(),
                    local_subscribers: vec![handler],
                    feedback_processor: ChannelFeedbackProcessor::new(channel),
                });
                true
            }
        };

        if maybe_sub && channel.source() != self.node_id {
            log::info!(
                "[PubsubRelayLogic {}] local sub {} event from {} pushed to list, new relay => send sub to source {}",
                self.node_id,
                channel,
                handler,
                channel.source()
            );
            self.output_events.push_back((channel.source(), None, PubsubRelayLogicOutput::Event(PubsubRemoteEvent::Sub(channel))));
            self.awaker.notify();
        }
    }

    /// This node unsubscribe that channle,
    /// then we must to check if no one subscribe that channel,
    /// if no one, we must to send a unsub event to the source node
    pub fn on_local_unsub(&mut self, channel: ChannelIdentify, handler: LocalSubId) {
        if let Some(slot) = self.channels.get_mut(&channel) {
            slot.feedback_processor.on_unsub(FeedbackConsumerId::Local(handler));
            if let Some(index) = slot.local_subscribers.iter().position(|&x| x == handler) {
                log::info!("[PubsubRelayLogic {}] local unsub {} event from {} removed from list", self.node_id, channel, handler);
                slot.local_subscribers.swap_remove(index);
            } else {
                log::info!("[PubsubRelayLogic {}] local unsub {} event from {} allready removed from list", self.node_id, channel, handler);
            }

            if slot.remote_subscribers.len() == 0 && slot.local_subscribers.len() == 0 {
                if let Some(info) = &slot.acked {
                    log::info!(
                        "[PubsubRelayLogic {}] local unsub {} event from {} list empty => send unsub to next node {}",
                        self.node_id,
                        channel,
                        handler,
                        info.from_node
                    );
                    self.output_events
                        .push_back((info.from_node, Some(info.from_conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::Unsub(channel))));
                    self.awaker.notify();
                } else {
                    if self.node_id != channel.source() {
                        log::warn!("[PubsubRelayLogic {}] local unsub {} event from {} list empty => but no next node", self.node_id, channel, handler);
                    } else {
                        log::info!(
                            "[PubsubRelayLogic {}] local unsub {} event from {} list empty in source node => removed",
                            self.node_id,
                            channel,
                            handler
                        );
                    }
                    self.channels.remove(&channel);
                }
            }
        } else {
            log::warn!("[PubsubRelayLogic {}] local unsub {} event from {} but no channel found", self.node_id, channel, handler);
        }
    }

    pub fn on_event(&mut self, now_ms: u64, from: NodeId, conn: ConnId, event: PubsubRemoteEvent) {
        match event {
            PubsubRemoteEvent::Sub(id) => {
                let maybe_sub = match self.channels.entry(id) {
                    Entry::Occupied(mut entry) => {
                        let value = entry.get_mut();
                        // if from conn is not in value.remotes list => push to list
                        // if from conn is in value.remotes list => do nothing
                        if !value.remote_subscribers.contains(&conn) {
                            value.remote_subscribers.push(conn);
                            log::info!("[PubsubRelayLogic {}] sub {} event from {} pushed to list", self.node_id, id, from);
                            self.output_events.push_back((from, Some(conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::SubAck(id, true))));
                        } else {
                            log::info!("[PubsubRelayLogic {}] sub {} event from {} allready added", self.node_id, id, from);
                            self.output_events.push_back((from, Some(conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::SubAck(id, false))));
                        }
                        value.remote_subscribers_ts.insert(conn, now_ms);
                        false
                    }
                    Entry::Vacant(entry) => {
                        log::info!("[PubsubRelayLogic {}] sub {} event from {} pushed to list, new relay", self.node_id, id, from);
                        entry.insert(ChannelContainer {
                            source: id.source(),
                            acked: None,
                            remote_subscribers: vec![conn],
                            remote_subscribers_ts: HashMap::from([(conn, now_ms)]),
                            local_subscribers: vec![],
                            feedback_processor: ChannelFeedbackProcessor::new(id),
                        });
                        self.output_events.push_back((from, Some(conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::SubAck(id, true))));
                        true
                    }
                };

                if maybe_sub && id.source() != self.node_id {
                    log::info!(
                        "[PubsubRelayLogic {}] sub {} event from {} pushed to list, new relay => send sub to source {}",
                        self.node_id,
                        id,
                        from,
                        id.source()
                    );
                    self.output_events.push_back((id.source(), None, PubsubRelayLogicOutput::Event(PubsubRemoteEvent::Sub(id))));
                }

                self.awaker.notify();
            }
            PubsubRemoteEvent::Unsub(id) => {
                if let Some(slot) = self.channels.get_mut(&id) {
                    slot.feedback_processor.on_unsub(FeedbackConsumerId::Remote(conn));
                    if let Some(index) = slot.remote_subscribers.iter().position(|&x| x == conn) {
                        slot.remote_subscribers.swap_remove(index);
                        log::info!("[PubsubRelayLogic {}] unsub {} event from {} removed from list", self.node_id, id, from);
                        self.output_events.push_back((from, Some(conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::UnsubAck(id, true))));
                    } else {
                        log::info!("[PubsubRelayLogic {}] unsub {} event from {} allready removed from list", self.node_id, id, from);
                        self.output_events.push_back((from, Some(conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::UnsubAck(id, false))));
                    }

                    if slot.remote_subscribers.len() == 0 && slot.local_subscribers.len() == 0 {
                        if slot.source != self.node_id {
                            if let Some(info) = &slot.acked {
                                log::info!(
                                    "[PubsubRelayLogic {}] unsub {} event from {} list empty => send unsub to next node {}",
                                    self.node_id,
                                    id,
                                    from,
                                    info.from_node
                                );
                                self.output_events
                                    .push_back((info.from_node, Some(info.from_conn), PubsubRelayLogicOutput::Event(PubsubRemoteEvent::Unsub(id))));
                            } else {
                                self.channels.remove(&id);
                                if id.source() != self.node_id {
                                    log::warn!("[PubsubRelayLogic {}] unsub {} event from {} list empty => but no next node", self.node_id, id, from);
                                } else {
                                    log::info!("[PubsubRelayLogic {}] unsub {} event from {} list empty in source node => removed", self.node_id, id, from);
                                }
                            }
                        } else {
                            self.channels.remove(&id);
                            log::info!("[PubsubRelayLogic {}] unsub {} event from {} list empty in source node => removed", self.node_id, id, from);
                        }
                    }
                    self.awaker.notify();
                } else {
                    log::warn!("[PubsubRelayLogic {}] unsub {} event from {} but no channel found", self.node_id, id, from);
                }
            }
            PubsubRemoteEvent::SubAck(id, _added) => {
                if let Some(slot) = self.channels.get_mut(&id) {
                    log::info!("[PubsubRelayLogic {}] sub_ack {} event from {}", self.node_id, id, from);
                    slot.acked = Some(AckedInfo {
                        from_node: from,
                        from_conn: conn,
                        at_ms: now_ms,
                    });
                } else {
                    log::warn!("[PubsubRelayLogic {}] sub_ack {} event from {} but channel not found", self.node_id, id, from);
                }
            }
            PubsubRemoteEvent::UnsubAck(id, _removed) => {
                if self.channels.remove(&id).is_some() {
                    log::info!("[PubsubRelayLogic {}] unsub_ack {} event from {}", self.node_id, id, from);
                } else {
                    log::warn!("[PubsubRelayLogic {}] unsub_ack {} event from {} but channel not found", self.node_id, id, from);
                }
            }
        }
    }

    pub fn relay(&self, channel: ChannelIdentify) -> Option<(&[ConnId], &[LocalSubId])> {
        log::trace!("[PubsubRelayLogic {}] relay channel {}", self.node_id, channel);
        let slot = self.channels.get(&channel)?;
        Some((&slot.remote_subscribers, &slot.local_subscribers))
    }

    pub fn pop_action(&mut self) -> Option<(NodeId, Option<ConnId>, PubsubRelayLogicOutput)> {
        self.output_events.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bluesea_identity::{ConnId, NodeId};
    use utils::awaker::{Awaker, MockAwaker};

    use crate::{
        msg::PubsubRemoteEvent,
        relay::{
            feedback::{Feedback, FeedbackConsumerId, FeedbackType, NumberInfo},
            ChannelIdentify, LocalSubId,
        },
        PUBSUB_CHANNEL_RESYNC_MS, PUBSUB_CHANNEL_TIMEOUT_MS,
    };

    use super::{PubsubRelayLogic, PubsubRelayLogicOutput};

    enum Event {
        Tick(u64, Vec<Feedback>),
        InLocalSub(ChannelIdentify, LocalSubId),
        InLocalUnsub(ChannelIdentify, LocalSubId),
        In(u64, NodeId, ConnId, PubsubRemoteEvent),
        InFb(u64, ChannelIdentify, FeedbackConsumerId, Feedback, Option<Feedback>),
        OutAwake(usize),
        OutNone,
        Out(NodeId, Option<ConnId>, PubsubRemoteEvent),
        OutFb(NodeId, Option<ConnId>, Feedback),
        Validate(Box<dyn FnOnce(&PubsubRelayLogic) -> bool>),
    }

    fn test(node_id: NodeId, events: Vec<Event>) {
        let awake = Arc::new(MockAwaker::default());
        let mut logic = PubsubRelayLogic::new(node_id);
        logic.set_awaker(awake.clone());

        for event in events {
            match event {
                Event::Tick(now_ms, local_fbs) => assert_eq!(logic.tick(now_ms), local_fbs),
                Event::InLocalSub(channel, handler) => logic.on_local_sub(channel, handler),
                Event::InLocalUnsub(channel, handler) => logic.on_local_unsub(channel, handler),
                Event::In(now_ms, from, conn, event) => logic.on_event(now_ms, from, conn, event),
                Event::InFb(now_ms, channel, consumer, fb, out) => assert_eq!(logic.on_feedback(now_ms, channel, consumer, fb), out),
                Event::OutAwake(count) => assert_eq!(awake.pop_awake_count(), count),
                Event::OutNone => assert_eq!(logic.pop_action(), None),
                Event::Out(from, conn, event) => assert_eq!(logic.pop_action(), Some((from, conn, PubsubRelayLogicOutput::Event(event)))),
                Event::OutFb(from, conn, fb) => assert_eq!(logic.pop_action(), Some((from, conn, PubsubRelayLogicOutput::Feedback(fb)))),
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

        let fb = Feedback {
            channel: channel,
            id: 1,
            feedback_type: FeedbackType::Passthrough(vec![1]),
        };
        let fb2 = Feedback {
            channel: channel,
            id: 2,
            feedback_type: FeedbackType::Number {
                window_ms: 100,
                info: NumberInfo { count: 1, max: 1, min: 1, sum: 1 },
            },
        };

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                //Test sub
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                //Test feedback
                Event::InFb(0, channel, FeedbackConsumerId::Local(1), fb.clone(), Some(fb.clone())),
                Event::OutNone,
                Event::InFb(0, channel, FeedbackConsumerId::Remote(remote_conn_id), fb.clone(), Some(fb.clone())),
                Event::OutNone,
                Event::OutAwake(0),
                Event::InFb(100, channel, FeedbackConsumerId::Remote(remote_conn_id), fb2.clone(), Some(fb2.clone())),
                Event::OutNone,
                Event::OutAwake(0),
                Event::InFb(150, channel, FeedbackConsumerId::Remote(remote_conn_id), fb2.clone(), None),
                Event::OutNone,
                Event::OutAwake(0),
                Event::Tick(200, vec![fb2.clone()]),
                Event::OutNone,
                Event::OutAwake(0),
                //Test unsub
                Event::In(200, remote_node_id, remote_conn_id, PubsubRemoteEvent::Unsub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }

    /// This test case ensure sub event to source node only generate sub ack
    #[test]
    fn in_source_remote_multi() {
        let node_id = 0;

        let channel = ChannelIdentify::new(111, node_id);

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        let remote_node_id2 = 2;
        let remote_conn_id2 = ConnId::from_in(10, 3);

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::In(0, remote_node_id2, remote_conn_id2, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id2, Some(remote_conn_id2), PubsubRemoteEvent::SubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id, remote_conn_id2].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::In(0, remote_node_id2, remote_conn_id2, PubsubRemoteEvent::Unsub(channel)),
                Event::Out(remote_node_id2, Some(remote_conn_id2), PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Unsub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::OutAwake(1),
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

        let fb = Feedback {
            channel: channel,
            id: 1,
            feedback_type: FeedbackType::Passthrough(vec![1]),
        };

        let fb2 = Feedback {
            channel: channel,
            id: 2,
            feedback_type: FeedbackType::Number {
                window_ms: 100,
                info: NumberInfo { count: 1, max: 1, min: 1, sum: 1 },
            },
        };

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                // Test sub
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::In(0, next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                // Test feedback
                Event::InFb(0, channel, FeedbackConsumerId::Local(1), fb.clone(), None),
                Event::OutFb(next_node_id, Some(next_conn_id), fb.clone()),
                Event::OutAwake(1),
                Event::InFb(0, channel, FeedbackConsumerId::Remote(remote_conn_id), fb.clone(), None),
                Event::OutFb(next_node_id, Some(next_conn_id), fb.clone()),
                Event::OutAwake(1),
                Event::InFb(100, channel, FeedbackConsumerId::Remote(remote_conn_id), fb2.clone(), None),
                Event::OutFb(next_node_id, Some(next_conn_id), fb2.clone()),
                Event::OutAwake(1),
                Event::InFb(150, channel, FeedbackConsumerId::Remote(remote_conn_id), fb2.clone(), None),
                Event::OutNone,
                Event::OutAwake(0),
                Event::Tick(200, vec![]),
                Event::OutFb(next_node_id, Some(next_conn_id), fb2.clone()),
                Event::OutAwake(0),
                // Test unsub
                Event::In(200, remote_node_id, remote_conn_id, PubsubRemoteEvent::Unsub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::Out(next_node_id, Some(next_conn_id), PubsubRemoteEvent::Unsub(channel)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::In(200, next_node_id, next_conn_id, PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::OutAwake(0),
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
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutAwake(0), //no need awake because of generated from tick
                Event::OutNone,
                Event::In(0, next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick(0, vec![]),
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
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::In(0, next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Tick(PUBSUB_CHANNEL_RESYNC_MS, vec![]),
                Event::Out(channel.source(), Some(next_conn_id), PubsubRemoteEvent::Sub(channel)),
                Event::OutAwake(0),
                Event::OutNone,
            ],
        );
    }

    #[test]
    fn in_source_auto_remove_timeout_subscribes() {
        let node_id = 0;

        let channel = ChannelIdentify::new(111, node_id);

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::Tick(PUBSUB_CHANNEL_TIMEOUT_MS, vec![]),
                Event::OutAwake(0),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }

    #[test]
    fn in_relay_auto_remove_timeout_subscribes() {
        let node_id = 0;

        let channel = ChannelIdentify::new(111, 1000);

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        let next_node_id = 2;
        let next_conn_id = ConnId::from_in(10, 3);

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::In(0, next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![].as_slice())));
                    true
                })),
                Event::Tick(PUBSUB_CHANNEL_TIMEOUT_MS, vec![]),
                Event::Out(next_node_id, Some(next_conn_id), PubsubRemoteEvent::Unsub(channel)),
                Event::OutAwake(0), //dont need awake because of genereated from tick
                Event::OutNone,
                Event::In(PUBSUB_CHANNEL_TIMEOUT_MS, next_node_id, next_conn_id, PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }

    #[test]
    fn in_source_local_simple() {
        let node_id = 0;

        let channel = ChannelIdentify::new(111, node_id);
        let handler = 1000;

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::InLocalSub(channel, handler),
                Event::OutAwake(0),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![].as_slice(), vec![handler].as_slice())));
                    true
                })),
                Event::InLocalUnsub(channel, handler),
                Event::OutAwake(0),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }

    #[test]
    fn in_relay_local_simple() {
        let node_id = 0;

        let channel = ChannelIdentify::new(111, 100);
        let handler = 1000;

        let next_node_id = 2;
        let next_conn_id = ConnId::from_in(10, 3);

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::InLocalSub(channel, handler),
                Event::OutAwake(1),
                Event::Out(channel.source(), None, PubsubRemoteEvent::Sub(channel)),
                Event::OutNone,
                Event::In(0, next_node_id, next_conn_id, PubsubRemoteEvent::SubAck(channel, true)),
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![].as_slice(), vec![handler].as_slice())));
                    true
                })),
                Event::InLocalUnsub(channel, handler),
                Event::OutAwake(1),
                Event::Out(next_node_id, Some(next_conn_id), PubsubRemoteEvent::Unsub(channel)),
                Event::OutNone,
                Event::In(0, next_node_id, next_conn_id, PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }

    /// This test case ensure sub event to source node only generate sub ack
    #[test]
    fn in_source_hybrid_multi() {
        let node_id = 0;

        let channel = ChannelIdentify::new(111, node_id);

        let handler = 1000;

        let remote_node_id = 1;
        let remote_conn_id = ConnId::from_in(10, 2);

        test(
            node_id,
            vec![
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::InLocalSub(channel, handler),
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Sub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::SubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Tick(0, vec![]),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), Some((vec![remote_conn_id].as_slice(), vec![handler].as_slice())));
                    true
                })),
                Event::InLocalUnsub(channel, handler),
                Event::In(0, remote_node_id, remote_conn_id, PubsubRemoteEvent::Unsub(channel)),
                Event::Out(remote_node_id, Some(remote_conn_id), PubsubRemoteEvent::UnsubAck(channel, true)),
                Event::OutAwake(1),
                Event::OutNone,
                Event::Validate(Box::new(move |logic| -> bool {
                    assert_eq!(logic.relay(channel), None);
                    true
                })),
            ],
        );
    }
}

use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::{RouteRule, ServiceBroadcastLevel};
use serde::{Deserialize, Serialize};

use crate::base::{Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, NetOutgoingMeta, Ttl};

pub const FEATURE_ID: u8 = 6;
pub const FEATURE_NAME: &str = "alias";
const HINT_TIMEOUT_MS: u64 = 2000;
const SCAN_TIMEOUT_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Register { alias: u64, service: u8, level: ServiceBroadcastLevel },
    Query { alias: u64, service: u8, level: ServiceBroadcastLevel },
    Unregister { alias: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundLocation {
    Local,
    RemoteHint(NodeId),
    RemoteScan(NodeId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    QueryResult(u64, Option<FoundLocation>),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ToWorker;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ToController;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Notify(u64),
    Scan(u64),
    Check(u64),
    Found(u64, bool),
}

enum QueryState {
    CheckHint(NodeId, u64),
    Scan(u64),
}

struct QuerySlot {
    waiters: Vec<FeatureControlActor>,
    state: QueryState,
    service: u8,
    level: ServiceBroadcastLevel,
}

#[derive(Default)]
pub struct AliasFeature {
    queries: HashMap<u64, QuerySlot>,
    hint_slots: HashMap<u64, NodeId>,
    local_slots: HashMap<u64, u64>,
    queue: VecDeque<FeatureOutput<Event, ToWorker>>,
    scan_seq: u16,
}

impl AliasFeature {
    fn process_controll(&mut self, now_ms: u64, actor: FeatureControlActor, control: Control) {
        match control {
            Control::Register { alias, service, level } => {
                log::info!("[AliasFeature] Register local alias {} and broadcast hint", alias);
                self.local_slots.insert(alias, now_ms);
                let seq = Self::gen_seq(&mut self.scan_seq);
                Self::send_to(&mut self.queue, RouteRule::ToServices(service, level, seq), Message::Notify(alias));
            }
            Control::Query { alias, service, level } => {
                if self.local_slots.contains_key(&alias) {
                    log::debug!("[AliasFeature] Found alias {} at local", alias);
                    self.queue.push_back(FeatureOutput::Event(actor, Event::QueryResult(alias, Some(FoundLocation::Local))));
                } else {
                    if let Some(slot) = self.queries.get_mut(&alias) {
                        log::debug!("[AliasFeature] Alias {} is already in query state => push to wait queue", alias);
                        slot.waiters.push(actor);
                    } else if let Some(node_id) = self.hint_slots.get(&alias) {
                        log::debug!("[AliasFeature] Alias {alias} is not in query state but has hint {node_id} => check hint");
                        self.queries.insert(
                            alias,
                            QuerySlot {
                                waiters: vec![actor],
                                state: QueryState::CheckHint(*node_id, now_ms),
                                service,
                                level,
                            },
                        );
                        Self::send_to(&mut self.queue, RouteRule::ToNode(*node_id), Message::Check(alias));
                    } else {
                        log::debug!("[AliasFeature] Alias {alias} is not in query state and has no hint => scan");
                        self.queries.insert(
                            alias,
                            QuerySlot {
                                waiters: vec![actor],
                                state: QueryState::Scan(now_ms),
                                service,
                                level,
                            },
                        );
                        let seq = Self::gen_seq(&mut self.scan_seq);
                        Self::send_to(&mut self.queue, RouteRule::ToServices(service, level, seq), Message::Scan(alias));
                    }
                }
            }
            Control::Unregister { alias } => {
                log::info!("[AliasFeature] Unregister alias {}", alias);
                self.local_slots.remove(&alias);
            }
        }
    }

    fn process_remote(&mut self, now_ms: u64, from: NodeId, msg: Message) {
        log::debug!("[AliasFeature] Received message from {from}: {:?}", msg);
        match msg {
            Message::Notify(alias) => {
                self.hint_slots.insert(alias, from);
            }
            Message::Scan(alias) => {
                if self.local_slots.contains_key(&alias) {
                    log::debug!("[AliasFeature] Received Scan alias {alias}, found at local");
                    Self::send_to(&mut self.queue, RouteRule::ToNode(from), Message::Found(alias, true));
                } else {
                    log::debug!("[AliasFeature] Received Scan alias {alias}, not found at local");
                }
            }
            Message::Check(alias) => {
                let found = self.local_slots.contains_key(&alias);
                log::debug!("[AliasFeature] Received Check alias {alias}, found at local: {found}");
                Self::send_to(&mut self.queue, RouteRule::ToNode(from), Message::Found(alias, found));
            }
            Message::Found(alias, found) => {
                if found {
                    self.hint_slots.insert(alias, from);
                }
                if let Some(slot) = self.queries.get_mut(&alias) {
                    match slot.state {
                        QueryState::CheckHint(node, _) => {
                            if node != from {
                                log::warn!("[AliasFeature] Reject Found message from wrong hint {node} vs {from}");
                                return;
                            }
                            if found {
                                log::debug!("[AliasFeature] Found alias {alias} at {node} => notify waiters {:?}", slot.waiters);
                                for actor in &slot.waiters {
                                    self.queue.push_back(FeatureOutput::Event(*actor, Event::QueryResult(alias, Some(FoundLocation::RemoteHint(from)))));
                                }
                                self.queries.remove(&alias);
                            } else {
                                log::debug!("[AliasFeature] Not found alias {alias} at hint {node} => switch to Scan");
                                let seq = self.scan_seq;
                                self.scan_seq = self.scan_seq.wrapping_add(1);
                                slot.state = QueryState::Scan(now_ms);
                                Self::send_to(&mut self.queue, RouteRule::ToServices(slot.service, slot.level, seq), Message::Scan(alias));
                            }
                        }
                        QueryState::Scan(_) => {
                            if !found {
                                log::warn!("[AliasFeature] Remote should not reply with Found=false for Scan");
                                return;
                            }
                            log::debug!("[AliasFeature] Found alias {alias} at {from} with Scan => notify waiters {:?}", slot.waiters);
                            for actor in &slot.waiters {
                                self.queue.push_back(FeatureOutput::Event(*actor, Event::QueryResult(alias, Some(FoundLocation::RemoteScan(from)))));
                            }
                            self.queries.remove(&alias);
                        }
                    }
                }
            }
        }
    }

    fn send_to(queue: &mut VecDeque<FeatureOutput<Event, ToWorker>>, rule: RouteRule, msg: Message) {
        let msg = bincode::serialize(&msg).expect("Should to bytes");
        queue.push_back(FeatureOutput::SendRoute(rule, NetOutgoingMeta::new(true, Ttl::default(), 0), msg));
    }

    fn gen_seq(scan_seq: &mut u16) -> u16 {
        let seq = *scan_seq;
        *scan_seq = scan_seq.wrapping_add(1);
        seq
    }
}

impl Feature<Control, Event, ToController, ToWorker> for AliasFeature {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, now: u64, input: FeatureSharedInput) {
        match input {
            FeatureSharedInput::Tick(_) => {
                let mut timeout = vec![];
                for (alias, slot) in &mut self.queries {
                    match &slot.state {
                        QueryState::CheckHint(hint, started_at) => {
                            if now >= *started_at + HINT_TIMEOUT_MS {
                                log::debug!("[AliasFeature] check {alias} hint node {hint} timeout => switch to Scan");

                                let seq = self.scan_seq;
                                self.scan_seq = self.scan_seq.wrapping_add(1);
                                slot.state = QueryState::Scan(now);
                                Self::send_to(&mut self.queue, RouteRule::ToServices(slot.service, slot.level, seq), Message::Scan(*alias));
                            }
                        }
                        QueryState::Scan(started_at) => {
                            if now == *started_at + SCAN_TIMEOUT_MS {
                                timeout.push(*alias);
                            }
                        }
                    }
                }

                for alias in timeout {
                    let slot = self.queries.remove(&alias).expect("Should have slot");
                    log::debug!("[AliasFeature] scan {alias} timeout => notify waiters {:?}", slot.waiters);
                    for actor in slot.waiters {
                        self.queue.push_back(FeatureOutput::Event(actor, Event::QueryResult(alias, None)));
                    }
                }
            }
            _ => {}
        }
    }

    fn on_input<'a>(&mut self, _ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => self.process_controll(now_ms, actor, control),
            FeatureInput::Local(meta, msg) | FeatureInput::Net(_, meta, msg) => match (meta.source, bincode::deserialize::<Message>(&msg)) {
                (Some(from), Ok(msg)) => self.process_remote(now_ms, from, msg),
                _ => {}
            },
            _ => {}
        }
    }

    fn pop_output(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<Event, ToWorker>> {
        self.queue.pop_front()
    }
}

#[derive(Default)]
pub struct AliasFeatureWorker;

impl FeatureWorker<Control, Event, ToController, ToWorker> for AliasFeatureWorker {}

#[cfg(test)]
mod tests {
    use atm0s_sdn_router::{RouteRule, ServiceBroadcastLevel};

    use crate::{
        base::{Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput},
        features::alias::{HINT_TIMEOUT_MS, SCAN_TIMEOUT_MS},
    };

    use super::{AliasFeature, Control, Event, FoundLocation, Message, ToWorker};

    fn decode_msg(msg: Option<FeatureOutput<Event, ToWorker>>) -> Option<(RouteRule, Message)> {
        match msg? {
            FeatureOutput::SendRoute(rule, _, msg) => Some((rule, bincode::deserialize(&msg).expect("Should decode"))),
            _ => panic!("Should be SendRoute"),
        }
    }

    #[test]
    fn local_alias_simple() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;
        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Register { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToServices(service, level, 0), Message::Notify(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Query { alias: 1000, service, level }));
        assert_eq!(
            alias.pop_output(&ctx),
            Some(FeatureOutput::Event(FeatureControlActor::Controller, Event::QueryResult(1000, Some(FoundLocation::Local))))
        );
        assert_eq!(alias.pop_output(&ctx), None);
    }

    #[test]
    fn local_alias_handle_check() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;
        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Register { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToServices(service, level, 0), Message::Notify(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        alias.process_remote(0, 123, Message::Check(1000));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToNode(123), Message::Found(1000, true))));
        assert_eq!(alias.pop_output(&ctx), None);

        alias.process_remote(0, 123, Message::Check(1001));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToNode(123), Message::Found(1001, false))));
        assert_eq!(alias.pop_output(&ctx), None);
    }

    #[test]
    fn local_alias_handle_scan() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;
        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Register { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToServices(service, level, 0), Message::Notify(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        alias.process_remote(0, 123, Message::Scan(1000));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToNode(123), Message::Found(1000, true))));
        assert_eq!(alias.pop_output(&ctx), None);

        alias.process_remote(0, 123, Message::Scan(1001));
        assert_eq!(alias.pop_output(&ctx), None);
    }

    #[test]
    fn found_remote_with_hint() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, 123);

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToNode(123), Message::Check(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate remote found
        alias.process_remote(100, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(&ctx),
            Some(FeatureOutput::Event(FeatureControlActor::Controller, Event::QueryResult(1000, Some(FoundLocation::RemoteHint(123)))))
        );
        assert_eq!(alias.pop_output(&ctx), None);
    }

    #[test]
    fn found_remote_with_scan() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate scan found
        alias.process_remote(100, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(&ctx),
            Some(FeatureOutput::Event(FeatureControlActor::Controller, Event::QueryResult(1000, Some(FoundLocation::RemoteScan(123)))))
        );
        assert_eq!(alias.pop_output(&ctx), None);
    }

    #[test]
    fn found_remote_with_hint_then_scan_fallback() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, 122);

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToNode(122), Message::Check(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate remote not found
        alias.process_remote(100, 122, Message::Found(1000, false));

        // will fallback to scan
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate scan found
        alias.process_remote(100, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(&ctx),
            Some(FeatureOutput::Event(FeatureControlActor::Controller, Event::QueryResult(1000, Some(FoundLocation::RemoteScan(123)))))
        );
        assert_eq!(alias.pop_output(&ctx), None);
    }

    #[test]
    fn found_remote_with_hint_timeout_then_scan_fallback() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, 122);

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToNode(122), Message::Check(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate remote not found
        alias.on_shared_input(&ctx, HINT_TIMEOUT_MS, FeatureSharedInput::Tick(0));

        // will fallback to scan
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate scan found
        alias.process_remote(100, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(&ctx),
            Some(FeatureOutput::Event(FeatureControlActor::Controller, Event::QueryResult(1000, Some(FoundLocation::RemoteScan(123)))))
        );
        assert_eq!(alias.pop_output(&ctx), None);

        //after that hint should be saved
        assert_eq!(alias.hint_slots.get(&1000), Some(&123));
    }

    #[test]
    fn timeout_both_hint_and_scan() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, 122);

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller, Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToNode(122), Message::Check(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate remote not found
        alias.on_shared_input(&ctx, HINT_TIMEOUT_MS, FeatureSharedInput::Tick(0));

        // will fallback to scan
        assert_eq!(decode_msg(alias.pop_output(&ctx)), Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000))));
        assert_eq!(alias.pop_output(&ctx), None);

        //simulate scan found
        alias.on_shared_input(&ctx, HINT_TIMEOUT_MS + SCAN_TIMEOUT_MS, FeatureSharedInput::Tick(1));

        assert_eq!(alias.pop_output(&ctx), Some(FeatureOutput::Event(FeatureControlActor::Controller, Event::QueryResult(1000, None))));
        assert_eq!(alias.pop_output(&ctx), None);
    }

    #[test]
    fn handle_notify_from_remote() {
        let mut alias = AliasFeature::default();
        alias.process_remote(100, 123, Message::Notify(1000));
        assert_eq!(alias.hint_slots.get(&1000), Some(&123));
    }
}

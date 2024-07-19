use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::{RouteRule, ServiceBroadcastLevel};
use derivative::Derivative;
use sans_io_runtime::{collections::DynamicDeque, TaskSwitcherChild};
use serde::{Deserialize, Serialize};

use crate::base::{Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerInput, FeatureWorkerOutput, NetOutgoingMeta, Ttl};

pub const FEATURE_ID: u8 = 6;
pub const FEATURE_NAME: &str = "alias";
pub const HINT_TIMEOUT_MS: u64 = 2000;
pub const SCAN_TIMEOUT_MS: u64 = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Register { alias: u64, service: u8, level: ServiceBroadcastLevel },
    Query { alias: u64, service: u8, level: ServiceBroadcastLevel },
    Unregister { alias: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FoundLocation {
    Local,
    Notify(NodeId),
    CachedHint(NodeId),
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

#[derive(Debug)]
enum QueryState {
    CheckHint(NodeId, u64),
    Scan(u64),
}

#[derive(Debug)]
struct QuerySlot<UserData> {
    waiters: Vec<FeatureControlActor<UserData>>,
    state: QueryState,
    service: u8,
    level: ServiceBroadcastLevel,
}

#[derive(Debug, PartialEq, Eq)]
struct HintSlot {
    node: NodeId,
    ts: u64,
}

pub type Output<UserData> = FeatureOutput<UserData, Event, ToWorker>;
pub type WorkerOutput<UserData> = FeatureWorkerOutput<UserData, Control, Event, ToController>;

#[derive(Debug, Derivative)]
#[derivative(Default(bound = ""))]
pub struct AliasFeature<UserData> {
    queries: HashMap<u64, QuerySlot<UserData>>,
    hint_slots: HashMap<u64, HintSlot>,
    local_slots: HashMap<u64, u64>,
    queue: VecDeque<Output<UserData>>,
    scan_seq: u16,
}

impl<UserData: Debug + Copy> AliasFeature<UserData> {
    fn process_control(&mut self, now_ms: u64, actor: FeatureControlActor<UserData>, control: Control) {
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
                } else if let Some(slot) = self.queries.get_mut(&alias) {
                    log::debug!("[AliasFeature] Alias {} is already in query state => push to wait queue", alias);
                    slot.waiters.push(actor);
                } else if let Some(slot) = self.hint_slots.get(&alias) {
                    if slot.ts + HINT_TIMEOUT_MS >= now_ms {
                        log::debug!("[AliasFeature] Alias {alias} is very newly added ({} vs now {}) to hint {} => reuse", slot.ts, now_ms, slot.node);
                        self.queue.push_back(FeatureOutput::Event(actor, Event::QueryResult(alias, Some(FoundLocation::CachedHint(slot.node)))));
                    } else {
                        log::debug!("[AliasFeature] Alias {alias} is not in query state but has hint {} => check hint", slot.node);
                        self.queries.insert(
                            alias,
                            QuerySlot {
                                waiters: vec![actor],
                                state: QueryState::CheckHint(slot.node, now_ms),
                                service,
                                level,
                            },
                        );
                        Self::send_to(&mut self.queue, RouteRule::ToNode(slot.node), Message::Check(alias));
                    }
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
                self.hint_slots.insert(alias, HintSlot { node: from, ts: now_ms });
                if let Some(slot) = self.queries.remove(&alias) {
                    for actor in &slot.waiters {
                        self.queue.push_back(FeatureOutput::Event(*actor, Event::QueryResult(alias, Some(FoundLocation::Notify(from)))));
                    }
                }
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
                    self.hint_slots.insert(alias, HintSlot { node: from, ts: now_ms });
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

    fn send_to(queue: &mut VecDeque<FeatureOutput<UserData, Event, ToWorker>>, rule: RouteRule, msg: Message) {
        let msg = bincode::serialize(&msg).expect("Should to bytes");
        queue.push_back(FeatureOutput::SendRoute(rule, NetOutgoingMeta::new(true, Ttl::default(), 0, true), msg.into()));
    }

    fn gen_seq(scan_seq: &mut u16) -> u16 {
        let seq = *scan_seq;
        *scan_seq = scan_seq.wrapping_add(1);
        seq
    }
}

impl<UserData: Debug + Copy> Feature<UserData, Control, Event, ToController, ToWorker> for AliasFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, now: u64, input: FeatureSharedInput) {
        if let FeatureSharedInput::Tick(_) = input {
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
                        if now >= *started_at + SCAN_TIMEOUT_MS {
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
    }

    fn on_input<'a>(&mut self, _ctx: &FeatureContext, now_ms: u64, input: FeatureInput<'a, UserData, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => self.process_control(now_ms, actor, control),
            FeatureInput::Local(meta, msg) | FeatureInput::Net(_, meta, msg) => {
                if !meta.secure {
                    log::warn!("[AliasFeature] reject unsecure message");
                    return;
                }
                if let (Some(from), Ok(msg)) = (meta.source, bincode::deserialize::<Message>(&msg)) {
                    self.process_remote(now_ms, from, msg)
                }
            }
            _ => {}
        }
    }
}

impl<UserData> TaskSwitcherChild<Output<UserData>> for AliasFeature<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<Output<UserData>> {
        self.queue.pop_front()
    }
}

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
pub struct AliasFeatureWorker<UserData> {
    queue: DynamicDeque<WorkerOutput<UserData>, 1>,
}

impl<UserData> FeatureWorker<UserData, Control, Event, ToController, ToWorker> for AliasFeatureWorker<UserData> {
    fn on_input(&mut self, _ctx: &mut crate::base::FeatureWorkerContext, _now: u64, input: crate::base::FeatureWorkerInput<UserData, Control, ToWorker>) {
        match input {
            FeatureWorkerInput::Control(actor, control) => self.queue.push_back(FeatureWorkerOutput::ForwardControlToController(actor, control)),
            FeatureWorkerInput::Network(conn, header, buf) => self.queue.push_back(FeatureWorkerOutput::ForwardNetworkToController(conn, header, buf)),
            #[cfg(feature = "vpn")]
            FeatureWorkerInput::TunPkt(..) => {}
            FeatureWorkerInput::FromController(..) => {
                log::warn!("No handler for FromController");
            }
            FeatureWorkerInput::Local(header, buf) => self.queue.push_back(FeatureWorkerOutput::ForwardLocalToController(header, buf)),
        }
    }
}

impl<UserData> TaskSwitcherChild<WorkerOutput<UserData>> for AliasFeatureWorker<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<WorkerOutput<UserData>> {
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use atm0s_sdn_router::{RouteRule, ServiceBroadcastLevel};
    use sans_io_runtime::TaskSwitcherChild;

    use crate::{
        base::{Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput},
        features::alias::{HintSlot, HINT_TIMEOUT_MS, SCAN_TIMEOUT_MS},
    };

    use super::{AliasFeature, Control, Event, FoundLocation, Message, ToWorker};

    fn decode_msg(msg: Option<FeatureOutput<(), Event, ToWorker>>) -> Option<(RouteRule, Message)> {
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
        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Register { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(0)), Some((RouteRule::ToServices(service, level, 0), Message::Notify(1000))));
        assert_eq!(alias.pop_output(0), None);

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Query { alias: 1000, service, level }));
        assert_eq!(
            alias.pop_output(0),
            Some(FeatureOutput::Event(FeatureControlActor::Controller(()), Event::QueryResult(1000, Some(FoundLocation::Local))))
        );
        assert_eq!(alias.pop_output(0), None);
    }

    #[test]
    fn local_alias_handle_check() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;
        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Register { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(0)), Some((RouteRule::ToServices(service, level, 0), Message::Notify(1000))));
        assert_eq!(alias.pop_output(0), None);

        alias.process_remote(0, 123, Message::Check(1000));
        assert_eq!(decode_msg(alias.pop_output(0)), Some((RouteRule::ToNode(123), Message::Found(1000, true))));
        assert_eq!(alias.pop_output(0), None);

        alias.process_remote(0, 123, Message::Check(1001));
        assert_eq!(decode_msg(alias.pop_output(0)), Some((RouteRule::ToNode(123), Message::Found(1001, false))));
        assert_eq!(alias.pop_output(0), None);
    }

    #[test]
    fn local_alias_handle_scan() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;
        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Register { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(0)), Some((RouteRule::ToServices(service, level, 0), Message::Notify(1000))));
        assert_eq!(alias.pop_output(0), None);

        alias.process_remote(0, 123, Message::Scan(1000));
        assert_eq!(decode_msg(alias.pop_output(0)), Some((RouteRule::ToNode(123), Message::Found(1000, true))));
        assert_eq!(alias.pop_output(0), None);

        alias.process_remote(0, 123, Message::Scan(1001));
        assert_eq!(alias.pop_output(0), None);
    }

    #[test]
    fn found_cached_hint() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, HintSlot { node: 123, ts: 0 });

        alias.on_input(
            &ctx,
            HINT_TIMEOUT_MS,
            FeatureInput::Control(FeatureControlActor::Controller(()), Control::Query { alias: 1000, service, level }),
        );

        assert_eq!(
            alias.pop_output(HINT_TIMEOUT_MS),
            Some(FeatureOutput::Event(
                FeatureControlActor::Controller(()),
                Event::QueryResult(1000, Some(FoundLocation::CachedHint(123)))
            ))
        );
        assert_eq!(alias.pop_output(HINT_TIMEOUT_MS), None);
    }

    #[test]
    fn found_remote_with_hint() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, HintSlot { node: 123, ts: 0 });

        alias.on_input(&ctx, 10000, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(10000)), Some((RouteRule::ToNode(123), Message::Check(1000))));
        assert_eq!(alias.pop_output(10000), None);

        //simulate remote found
        alias.process_remote(10100, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(10100),
            Some(FeatureOutput::Event(
                FeatureControlActor::Controller(()),
                Event::QueryResult(1000, Some(FoundLocation::RemoteHint(123)))
            ))
        );
        assert_eq!(alias.pop_output(10100), None);
    }

    #[test]
    fn found_remote_with_scan() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.on_input(&ctx, 0, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(0)), Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000))));
        assert_eq!(alias.pop_output(0), None);

        //simulate scan found
        alias.process_remote(100, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(100),
            Some(FeatureOutput::Event(
                FeatureControlActor::Controller(()),
                Event::QueryResult(1000, Some(FoundLocation::RemoteScan(123)))
            ))
        );
        assert_eq!(alias.pop_output(100), None);
    }

    #[test]
    fn found_remote_with_hint_then_scan_fallback() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, HintSlot { node: 122, ts: 0 });

        alias.on_input(&ctx, 10000, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(10000)), Some((RouteRule::ToNode(122), Message::Check(1000))));
        assert_eq!(alias.pop_output(10000), None);

        //simulate remote not found
        alias.process_remote(10100, 122, Message::Found(1000, false));

        // will fallback to scan
        assert_eq!(decode_msg(alias.pop_output(10100)), Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000))));
        assert_eq!(alias.pop_output(10100), None);

        //simulate scan found
        alias.process_remote(10100, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(10100),
            Some(FeatureOutput::Event(
                FeatureControlActor::Controller(()),
                Event::QueryResult(1000, Some(FoundLocation::RemoteScan(123)))
            ))
        );
        assert_eq!(alias.pop_output(10100), None);
    }

    #[test]
    fn found_remote_with_hint_timeout_then_scan_fallback() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, HintSlot { node: 122, ts: 0 });

        alias.on_input(&ctx, 10000, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(10000)), Some((RouteRule::ToNode(122), Message::Check(1000))));
        assert_eq!(alias.pop_output(10000), None);

        //simulate remote not found
        alias.on_shared_input(&ctx, 10000 + HINT_TIMEOUT_MS, FeatureSharedInput::Tick(0));

        // will fallback to scan
        assert_eq!(
            decode_msg(alias.pop_output(10000 + HINT_TIMEOUT_MS)),
            Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000)))
        );
        assert_eq!(alias.pop_output(10000 + HINT_TIMEOUT_MS), None);

        //simulate scan found
        alias.process_remote(10100 + HINT_TIMEOUT_MS, 123, Message::Found(1000, true));

        assert_eq!(
            alias.pop_output(10100 + HINT_TIMEOUT_MS),
            Some(FeatureOutput::Event(
                FeatureControlActor::Controller(()),
                Event::QueryResult(1000, Some(FoundLocation::RemoteScan(123)))
            ))
        );
        assert_eq!(alias.pop_output(10100 + HINT_TIMEOUT_MS), None);

        //after that hint should be saved
        assert_eq!(
            alias.hint_slots.get(&1000),
            Some(&HintSlot {
                node: 123,
                ts: 10100 + HINT_TIMEOUT_MS
            })
        );
    }

    #[test]
    fn timeout_both_hint_and_scan() {
        let mut alias = AliasFeature::default();
        let ctx = FeatureContext { node_id: 0, session: 0 };
        let service = 1;
        let level = ServiceBroadcastLevel::Global;

        alias.hint_slots.insert(1000, HintSlot { node: 122, ts: 0 });

        alias.on_input(&ctx, 10000, FeatureInput::Control(FeatureControlActor::Controller(()), Control::Query { alias: 1000, service, level }));
        assert_eq!(decode_msg(alias.pop_output(10000)), Some((RouteRule::ToNode(122), Message::Check(1000))));
        assert_eq!(alias.pop_output(10000), None);

        //simulate remote not found
        alias.on_shared_input(&ctx, 10000 + HINT_TIMEOUT_MS, FeatureSharedInput::Tick(0));

        // will fallback to scan
        assert_eq!(
            decode_msg(alias.pop_output(10000 + HINT_TIMEOUT_MS)),
            Some((RouteRule::ToServices(service, level, 0), Message::Scan(1000)))
        );
        assert_eq!(alias.pop_output(10000 + HINT_TIMEOUT_MS), None);

        //simulate scan found
        alias.on_shared_input(&ctx, 10000 + HINT_TIMEOUT_MS + SCAN_TIMEOUT_MS, FeatureSharedInput::Tick(1));

        assert_eq!(
            alias.pop_output(10000 + HINT_TIMEOUT_MS + SCAN_TIMEOUT_MS),
            Some(FeatureOutput::Event(FeatureControlActor::Controller(()), Event::QueryResult(1000, None)))
        );
        assert_eq!(alias.pop_output(10000 + HINT_TIMEOUT_MS + SCAN_TIMEOUT_MS), None);
    }

    #[test]
    fn handle_notify_from_remote() {
        let mut alias = AliasFeature::<()>::default();
        alias.process_remote(100, 123, Message::Notify(1000));
        assert_eq!(alias.hint_slots.get(&1000), Some(&HintSlot { node: 123, ts: 100 }));
    }
}

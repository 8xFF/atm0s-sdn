use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Debug,
    net::SocketAddr,
};

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_router::{RouteRule, ServiceBroadcastLevel};
use serde::{Deserialize, Serialize};

use crate::{
    base::{ConnectionEvent, Service, ServiceBuilder, ServiceControlActor, ServiceInput, ServiceOutput, ServiceSharedInput, ServiceWorker, Ttl},
    features::{data, FeaturesControl, FeaturesEvent},
};

pub const SERVICE_ID: u8 = 1;
pub const SERVICE_NAME: &str = "manual_discovery";

const NODE_TIMEOUT_MS: u64 = 10000; // after 10 seconds of no ping, node is considered dead
const NODE_PING_MS: u64 = 5000;
const NODE_PING_TTL: u8 = 5;

fn data_cmd<SE, TW>(cmd: data::Control) -> ServiceOutput<FeaturesControl, SE, TW> {
    ServiceOutput::FeatureControl(FeaturesControl::Data(cmd))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionInfo {
    pub conn: ConnId,
    pub dest: NodeId,
    pub remote: SocketAddr,
    pub rtt_ms: u32,
}

struct NodeInfo {
    last_ping_ms: u64,
    conns: Vec<ConnectionInfo>,
}

#[derive(Debug, Clone)]
pub enum Control {
    Subscribe,
    GetAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    GotAll(Vec<(NodeId, Vec<ConnectionInfo>)>),
    NodeChanged(NodeId, Vec<ConnectionInfo>),
    NodeRemoved(NodeId),
}

#[derive(Debug, Serialize, Deserialize)]
enum Message {
    Snapshot(NodeId, Vec<ConnectionInfo>),
}

pub struct VisualizationService<SC, SE, TC, TW> {
    node_id: NodeId,
    last_ping: u64,
    broadcast_seq: u16,
    queue: VecDeque<ServiceOutput<FeaturesControl, SE, TW>>,
    conns: BTreeMap<ConnId, ConnectionInfo>,
    network_nodes: BTreeMap<NodeId, NodeInfo>,
    subscribers: Vec<ServiceControlActor>,
    _tmp: std::marker::PhantomData<(SC, TC, TW)>,
}

impl<SC, SE, TC, TW> VisualizationService<SC, SE, TC, TW>
where
    SC: From<Control> + TryInto<Control>,
    SE: From<Event> + TryInto<Event>,
{
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            broadcast_seq: 0,
            last_ping: 0,
            conns: BTreeMap::new(),
            network_nodes: BTreeMap::new(),
            queue: VecDeque::new(),
            subscribers: Vec::new(),
            _tmp: std::marker::PhantomData,
        }
    }

    fn fire_event(&mut self, event: Event) {
        for sub in self.subscribers.iter() {
            self.queue.push_back(ServiceOutput::Event(*sub, event.clone().into()));
        }
    }
}

impl<SC, SE, TC, TW> Service<FeaturesControl, FeaturesEvent, SC, SE, TC, TW> for VisualizationService<SC, SE, TC, TW>
where
    SC: From<Control> + TryInto<Control>,
    SE: From<Event> + TryInto<Event>,
{
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn on_shared_input<'a>(&mut self, now: u64, input: ServiceSharedInput) {
        match input {
            ServiceSharedInput::Tick(_) => {
                let mut to_remove = Vec::new();
                for (node, info) in self.network_nodes.iter() {
                    if now >= NODE_TIMEOUT_MS + info.last_ping_ms {
                        log::warn!("[Visualization] Node {} is dead after timeout {NODE_TIMEOUT_MS} ms", node);
                        to_remove.push(*node);
                    }
                }
                for node in to_remove {
                    self.fire_event(Event::NodeRemoved(node));
                    self.network_nodes.remove(&node);
                }

                if now >= self.last_ping + NODE_PING_MS {
                    log::debug!("[Visualization] Sending Snapshot to collector with interval {NODE_PING_MS} ms with {} conns", self.conns.len());
                    self.last_ping = now;
                    let msg = Message::Snapshot(self.node_id, self.conns.values().cloned().collect::<Vec<_>>());
                    let seq = self.broadcast_seq;
                    self.broadcast_seq = self.broadcast_seq.saturating_add(1);
                    self.queue.push_back(data_cmd(data::Control::SendRule(
                        RouteRule::ToServices(SERVICE_ID, ServiceBroadcastLevel::Global, seq),
                        Ttl(NODE_PING_TTL),
                        bincode::serialize(&msg).expect("Should to bytes"),
                    )));
                }
            }
            ServiceSharedInput::Connection(ConnectionEvent::Connected(ctx, _)) => {
                log::info!("[Visualization] New connection from {} to {}, set default rtt_ms to 1000ms", ctx.remote, ctx.node);
                self.conns.insert(
                    ctx.conn,
                    ConnectionInfo {
                        conn: ctx.conn,
                        dest: ctx.node,
                        remote: ctx.remote,
                        rtt_ms: 1000,
                    },
                );
            }
            ServiceSharedInput::Connection(ConnectionEvent::Stats(ctx, stats)) => {
                log::debug!("[Visualization] Update rtt_ms for connection from {} to {} to {}ms", ctx.remote, ctx.node, stats.rtt_ms);
                let entry = self.conns.entry(ctx.conn).or_insert(ConnectionInfo {
                    conn: ctx.conn,
                    dest: ctx.node,
                    remote: ctx.remote,
                    rtt_ms: 1000,
                });
                entry.rtt_ms = stats.rtt_ms;
            }
            ServiceSharedInput::Connection(ConnectionEvent::Disconnected(ctx)) => {
                log::info!("[Visualization] Connection from {} to {} is disconnected", ctx.remote, ctx.node);
                self.conns.remove(&ctx.conn);
            }
        }
    }

    fn on_input(&mut self, now: u64, input: ServiceInput<FeaturesEvent, SC, TC>) {
        match input {
            ServiceInput::FeatureEvent(FeaturesEvent::Data(data::Event::Recv(buf))) => {
                if let Ok(msg) = bincode::deserialize::<Message>(&buf) {
                    match msg {
                        Message::Snapshot(from, conns) => {
                            log::debug!("[Visualization] Got snapshot from {} with {} connections", from, conns.len());
                            self.fire_event(Event::NodeChanged(from, conns.clone()));
                            self.network_nodes.insert(from, NodeInfo { last_ping_ms: now, conns });
                        }
                    }
                }
            }
            ServiceInput::Control(actor, control) => {
                let mut push_all = || {
                    let all = self.network_nodes.iter().map(|(k, v)| (*k, v.conns.clone())).collect();
                    self.queue.push_back(ServiceOutput::Event(actor, Event::GotAll(all).into()));
                };
                if let Ok(control) = control.try_into() {
                    match control {
                        Control::GetAll => {
                            push_all();
                        }
                        Control::Subscribe => {
                            if !self.subscribers.contains(&actor) {
                                self.subscribers.push(actor);
                                log::info!("[Visualization] New subscriber, sending snapshot with {} nodes", self.network_nodes.len());
                                push_all();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn pop_output(&mut self) -> Option<ServiceOutput<FeaturesControl, SE, TW>> {
        self.queue.pop_front()
    }
}

pub struct VisualizationServiceWorker {}

impl<SE, TC, TW> ServiceWorker<FeaturesControl, FeaturesEvent, SE, TC, TW> for VisualizationServiceWorker {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }
}

pub struct VisualizationServiceBuilder<SC, SE, TC, TW> {
    collector: bool,
    node_id: NodeId,
    _tmp: std::marker::PhantomData<(SC, SE, TC, TW)>,
}

impl<SC, SE, TC, TW> VisualizationServiceBuilder<SC, SE, TC, TW> {
    pub fn new(collector: bool, node_id: NodeId) -> Self {
        log::info!("[Visualization] started as collector node => will receive metric from all other nodes");
        Self {
            collector,
            node_id,
            _tmp: std::marker::PhantomData,
        }
    }
}

impl<SC, SE, TC, TW> ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW> for VisualizationServiceBuilder<SC, SE, TC, TW>
where
    SC: 'static + Debug + Send + Sync + From<Control> + TryInto<Control>,
    SE: 'static + Debug + Send + Sync + From<Event> + TryInto<Event>,
    TC: 'static + Debug + Send + Sync,
    TW: 'static + Debug + Send + Sync,
{
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn discoverable(&self) -> bool {
        self.collector
    }

    fn create(&self) -> Box<dyn Service<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>> {
        Box::new(VisualizationService::new(self.node_id))
    }

    fn create_worker(&self) -> Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, SE, TC, TW>> {
        Box::new(VisualizationServiceWorker {})
    }
}

#[cfg(test)]
mod test {
    use std::net::SocketAddr;

    use atm0s_sdn_identity::{ConnId, NodeId};
    use atm0s_sdn_router::{RouteRule, ServiceBroadcastLevel};

    use crate::{
        base::{ConnectionCtx, ConnectionEvent, SecureContext, Service, ServiceInput, ServiceSharedInput, Ttl},
        features::{
            data::{Control as DataControl, Event as DataEvent},
            FeaturesEvent,
        },
        services::visualization::{data_cmd, Message, NODE_PING_MS, NODE_PING_TTL, NODE_TIMEOUT_MS},
    };

    use super::{Control, Event, VisualizationService, SERVICE_ID};

    fn data_event(event: DataEvent) -> ServiceInput<FeaturesEvent, Control, ()> {
        ServiceInput::FeatureEvent(FeaturesEvent::Data(event))
    }

    fn connected_event(node: NodeId) -> ConnectionEvent {
        ConnectionEvent::Connected(
            ConnectionCtx {
                conn: ConnId::from_in(0, node as u64),
                node,
                remote: SocketAddr::from(([127, 0, 0, 1], 1234)),
            },
            SecureContext {},
        )
    }

    fn disconnected_event(node: NodeId) -> ConnectionEvent {
        ConnectionEvent::Disconnected(ConnectionCtx {
            conn: ConnId::from_in(0, node as u64),
            node,
            remote: SocketAddr::from(([127, 0, 0, 1], 1234)),
        })
    }

    #[test]
    fn agent_should_prediotic_sending_snapshot() {
        let node_id = 1;
        let mut service = VisualizationService::<Control, Event, (), ()>::new(node_id);

        assert_eq!(service.pop_output(), None);

        service.on_shared_input(NODE_PING_MS, ServiceSharedInput::Tick(0));
        assert_eq!(
            service.pop_output(),
            Some(data_cmd(DataControl::SendRule(
                RouteRule::ToServices(SERVICE_ID, ServiceBroadcastLevel::Global, 0),
                Ttl(NODE_PING_TTL),
                bincode::serialize(&Message::Snapshot(node_id, vec![])).expect("Should to bytes")
            )))
        );

        service.on_shared_input(NODE_PING_MS * 2, ServiceSharedInput::Tick(0));
        assert_eq!(
            service.pop_output(),
            Some(data_cmd(DataControl::SendRule(
                RouteRule::ToServices(SERVICE_ID, ServiceBroadcastLevel::Global, 1),
                Ttl(NODE_PING_TTL),
                bincode::serialize(&Message::Snapshot(node_id, vec![])).expect("Should to bytes")
            )))
        );
    }

    #[test]
    fn agent_handle_connection_event() {
        let node_id = 1;
        let mut service = VisualizationService::<Control, Event, (), ()>::new(node_id);

        let node2 = 2;
        let node3 = 3;

        service.on_shared_input(110, ServiceSharedInput::Connection(connected_event(node2)));
        service.on_shared_input(110, ServiceSharedInput::Connection(connected_event(node3)));

        assert_eq!(service.conns.len(), 2);

        service.on_shared_input(210, ServiceSharedInput::Connection(disconnected_event(node3)));
        assert_eq!(service.conns.len(), 1);

        //TODO check with Snapshot msg too
    }

    #[test]
    fn collector_handle_snapshot_correct() {
        let node_id = 1;
        let mut service = VisualizationService::<Control, Event, (), ()>::new(node_id);

        let node2 = 2;

        let snapshot = Message::Snapshot(node2, vec![]);
        let buf = bincode::serialize(&snapshot).expect("Should to bytes");
        service.on_input(100, data_event(DataEvent::Recv(buf)));

        assert_eq!(service.network_nodes.len(), 1);

        //auto delete after timeout
        service.on_shared_input(100 + NODE_TIMEOUT_MS, ServiceSharedInput::Tick(0));
        assert_eq!(service.network_nodes.len(), 0);
    }
}

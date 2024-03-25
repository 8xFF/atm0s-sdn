use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;

use crate::base::{Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, NetOutgoingMeta};

pub const FEATURE_ID: u8 = 7;
pub const FEATURE_NAME: &str = "socket";

const MTU_SIZE: usize = 1300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Bind(u16),
    Connect(u16, NodeId, u16),
    SendTo(u16, NodeId, u16, Vec<u8>),
    Send(u16, Vec<u8>),
    Unbind(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    RecvFrom(u16, NodeId, u16, Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ToWorker;

#[derive(Debug, Clone)]
pub struct ToController;

struct Socket {
    target: Option<(NodeId, u16)>,
    actor: FeatureControlActor,
}

pub struct SocketFeature {
    sockets: HashMap<u16, Socket>,
    temp: [u8; MTU_SIZE],
    queue: VecDeque<FeatureOutput<Event, ToWorker>>,
}

impl SocketFeature {
    fn send_to(&mut self, src: u16, dest_node: NodeId, dest_port: u16, data: Vec<u8>) {
        if data.len() > MTU_SIZE - 4 {
            log::warn!("[SocketFeature] SendTo failed, data too large: {}", data.len());
            return;
        }

        self.temp[..2].copy_from_slice(&src.to_be_bytes());
        self.temp[2..4].copy_from_slice(&dest_port.to_be_bytes());
        self.temp[4..(4 + data.len())].copy_from_slice(&data);
        let meta: NetOutgoingMeta = NetOutgoingMeta::new(true, Default::default(), 0);
        self.queue
            .push_back(FeatureOutput::SendRoute(RouteRule::ToNode(dest_node), meta, self.temp[..(4 + data.len())].to_vec()));
    }
}

impl Default for SocketFeature {
    fn default() -> Self {
        Self {
            sockets: HashMap::new(),
            temp: [0; MTU_SIZE],
            queue: VecDeque::new(),
        }
    }
}

impl Feature<Control, Event, ToController, ToWorker> for SocketFeature {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, _input: FeatureSharedInput) {}

    fn on_input<'a>(&mut self, ctx: &FeatureContext, _now_ms: u64, input: FeatureInput<'a, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => match control {
                Control::Bind(port) => {
                    if self.sockets.contains_key(&port) {
                        log::warn!("[SocketFeature] Bind failed, port already in use: {}", port);
                        return;
                    }
                    self.sockets.insert(port, Socket { target: None, actor });
                }
                Control::Connect(port, dest_node, dest_port) => {
                    if let Some(socket) = self.sockets.get_mut(&port) {
                        socket.target = Some((dest_node, dest_port));
                    } else {
                        log::warn!("[SocketFeature] Connect failed, port not found: {}", port);
                    }
                }
                Control::SendTo(port, dest_node, dest_port, data) => {
                    if dest_node == ctx.node_id {
                        if self.sockets.contains_key(&dest_port) {
                            self.queue.push_back(FeatureOutput::Event(actor, Event::RecvFrom(dest_port, ctx.node_id, port, data)));
                        } else {
                            log::warn!("[SocketFeature] SendTo failed, port not found: {}", dest_port);
                        }
                    } else {
                        self.send_to(port, dest_node, dest_port, data);
                    }
                }
                Control::Send(port, data) => {
                    if let Some(socket) = self.sockets.get(&port) {
                        if let Some((dest_node, dest_port)) = socket.target {
                            if data.len() > MTU_SIZE - 4 {
                                log::warn!("[SocketFeature] Send failed, data too large: {}", data.len());
                                return;
                            }
                            self.send_to(port, dest_node, dest_port, data);
                        } else {
                            log::warn!("[SocketFeature] Send failed, target not found: {}", port);
                        }
                    } else {
                        log::warn!("[SocketFeature] Send failed, port not found: {}", port);
                    }
                }
                Control::Unbind(port) => {
                    if self.sockets.remove(&port).is_none() {
                        log::warn!("[SocketFeature] Unbind failed, port not found: {}", port);
                    }
                }
            },
            FeatureInput::Net(_, meta, data) | FeatureInput::Local(meta, data) => {
                let from_node = if let Some(source) = meta.source {
                    source
                } else {
                    log::warn!("[SocketFeature] Recv failed, source not set");
                    return;
                };
                let pkt_src = u16::from_be_bytes([data[0], data[1]]);
                let pkt_dest = u16::from_be_bytes([data[2], data[3]]);
                if let Some(socket) = self.sockets.get(&pkt_dest) {
                    if let Some((dest_node, dest_port)) = socket.target {
                        if dest_node != from_node {
                            log::warn!("[SocketFeature] Recv failed, node mismatch: {} != {}", dest_node, from_node);
                            return;
                        }
                        if dest_port != pkt_dest {
                            log::warn!("[SocketFeature] Recv failed, port mismatch: {} != {}", dest_port, pkt_dest);
                            return;
                        }
                        self.queue
                            .push_back(FeatureOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, data[4..].to_vec())));
                    } else {
                        self.queue
                            .push_back(FeatureOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, data[4..].to_vec())));
                    }
                } else {
                    log::warn!("[SocketFeature] Recv failed, port not found: {}", pkt_dest);
                }
            }
            _ => {}
        }
    }

    fn pop_output(&mut self, _ctx: &FeatureContext) -> Option<FeatureOutput<Event, ToWorker>> {
        self.queue.pop_front()
    }
}

#[derive(Debug, Default)]
pub struct SocketFeatureWorker;

impl FeatureWorker<Control, Event, ToController, ToWorker> for SocketFeatureWorker {}

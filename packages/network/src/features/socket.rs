use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    ops::Deref,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use sans_io_runtime::{collections::DynamicDeque, return_if_none, TaskSwitcherChild};

use crate::base::{
    Buffer, Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput,
    NetOutgoingMeta, Ttl,
};

pub const FEATURE_ID: u8 = 7;
pub const FEATURE_NAME: &str = "socket";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Bind(u16),
    Connect(u16, NodeId, u16),
    SendTo(u16, NodeId, u16, Buffer, u8),
    Send(u16, Buffer, u8),
    Unbind(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    RecvFrom(u16, NodeId, u16, Buffer, u8),
}

#[derive(Debug, Clone)]
pub enum ToWorker<UserData> {
    BindSocket(u16, FeatureControlActor<UserData>),
    ConnectSocket(u16, NodeId, u16),
    UnbindSocket(u16),
}

#[derive(Debug, Clone)]
pub struct ToController;

struct Socket<UserData> {
    target: Option<(NodeId, u16)>,
    actor: FeatureControlActor<UserData>,
}

pub type Output<UserData> = FeatureOutput<UserData, Event, ToWorker<UserData>>;
pub type WorkerOutput<UserData> = FeatureWorkerOutput<UserData, Control, Event, ToController>;

pub struct SocketFeature<UserData> {
    sockets: HashMap<u16, Socket<UserData>>,
    queue: VecDeque<Output<UserData>>,
}

impl<UserData> SocketFeature<UserData> {
    fn send_to(&mut self, src: u16, dest_node: NodeId, dest_port: u16, mut data: Buffer, meta: u8) {
        embed_meta(src, dest_port, &mut data);
        let meta: NetOutgoingMeta = NetOutgoingMeta::new(true, Default::default(), meta, false);
        self.queue.push_back(FeatureOutput::SendRoute(RouteRule::ToNode(dest_node), meta, data));
    }
}

impl<UserData> Default for SocketFeature<UserData> {
    fn default() -> Self {
        Self {
            sockets: HashMap::new(),
            queue: VecDeque::new(),
        }
    }
}

impl<UserData: Copy + Debug + Eq> Feature<UserData, Control, Event, ToController, ToWorker<UserData>> for SocketFeature<UserData> {
    fn on_shared_input(&mut self, _ctx: &FeatureContext, _now: u64, _input: FeatureSharedInput) {}

    fn on_input(&mut self, ctx: &FeatureContext, _now_ms: u64, input: FeatureInput<'_, UserData, Control, ToController>) {
        match input {
            FeatureInput::Control(actor, control) => match control {
                Control::Bind(port) => {
                    if self.sockets.contains_key(&port) {
                        log::warn!("[SocketFeature] Bind failed, port already in use: {}", port);
                        return;
                    }
                    self.sockets.insert(port, Socket { target: None, actor });
                    self.queue.push_back(FeatureOutput::ToWorker(true, ToWorker::BindSocket(port, actor)));
                }
                Control::Connect(port, dest_node, dest_port) => {
                    if let Some(socket) = self.sockets.get_mut(&port) {
                        if socket.actor == actor {
                            socket.target = Some((dest_node, dest_port));
                            self.queue.push_back(FeatureOutput::ToWorker(true, ToWorker::ConnectSocket(port, dest_node, dest_port)));
                        } else {
                            log::warn!("[SocketFeature] Connect failed, actor mismatch: {:?} != {:?}", socket.actor, actor);
                        }
                    } else {
                        log::warn!("[SocketFeature] Connect failed, port not found: {}", port);
                    }
                }
                Control::SendTo(port, dest_node, dest_port, data, meta) => {
                    if let Some(socket) = self.sockets.get(&port) {
                        if socket.actor == actor {
                            if dest_node == ctx.node_id {
                                if self.sockets.contains_key(&dest_port) {
                                    self.queue.push_back(FeatureOutput::Event(actor, Event::RecvFrom(dest_port, ctx.node_id, port, data, meta)));
                                } else {
                                    log::warn!("[SocketFeature] SendTo failed, port not found: {}", dest_port);
                                }
                            } else {
                                self.send_to(port, dest_node, dest_port, data, meta);
                            }
                        } else {
                            log::warn!("[SocketFeature] SendTo failed, actor mismatch: {:?} != {:?}", socket.actor, actor);
                        }
                    } else {
                        log::warn!("[SocketFeature] SendTo failed, port not found: {}", port);
                    }
                }
                Control::Send(port, data, meta) => {
                    if let Some(socket) = self.sockets.get(&port) {
                        if let Some((dest_node, dest_port)) = socket.target {
                            if dest_node == ctx.node_id {
                                if self.sockets.contains_key(&dest_port) {
                                    self.queue.push_back(FeatureOutput::Event(actor, Event::RecvFrom(dest_port, ctx.node_id, port, data, meta)));
                                } else {
                                    log::warn!("[SocketFeature] SendTo failed, port not found: {}", dest_port);
                                }
                            } else {
                                self.send_to(port, dest_node, dest_port, data, meta);
                            }
                        } else {
                            log::warn!("[SocketFeature] Send failed, target not found: {}", port);
                        }
                    } else {
                        log::warn!("[SocketFeature] Send failed, port not found: {}", port);
                    }
                }
                Control::Unbind(port) => {
                    if let Some(socket) = self.sockets.get(&port) {
                        if socket.actor == actor {
                            self.sockets.remove(&port);
                            self.queue.push_back(FeatureOutput::ToWorker(true, ToWorker::UnbindSocket(port)));
                        } else {
                            log::warn!("[SocketFeature] Unbind failed, actor mismatch: {:?} != {:?}", socket.actor, actor);
                        }
                    } else {
                        log::warn!("[SocketFeature] Unbind failed, port not found: {}", port);
                    }
                }
            },
            FeatureInput::Net(_, meta, mut buf) | FeatureInput::Local(meta, mut buf) => {
                let from_node = if let Some(source) = meta.source {
                    source
                } else {
                    log::warn!("[SocketFeature] Recv failed, source not set");
                    return;
                };
                let (pkt_src, pkt_dest) = if let Some(res) = extract_meta(&mut buf) {
                    res
                } else {
                    log::warn!("[SocketFeature] Recv failed, invalid data");
                    return;
                };
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
                        self.queue.push_back(FeatureOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, buf, meta.meta)));
                    } else {
                        self.queue.push_back(FeatureOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, buf, meta.meta)));
                    }
                } else {
                    log::warn!("[SocketFeature] Recv failed, port not found: {}", pkt_dest);
                }
            }
            _ => {}
        }
    }
}

impl<UserData> TaskSwitcherChild<Output<UserData>> for SocketFeature<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<Output<UserData>> {
        self.queue.pop_front()
    }
}

pub struct SocketFeatureWorker<UserData> {
    sockets: HashMap<u16, Socket<UserData>>,
    queue: DynamicDeque<WorkerOutput<UserData>, 16>,
}

impl<UserData: Copy> SocketFeatureWorker<UserData> {
    fn process_incoming(&mut self, from_node: NodeId, mut buf: Buffer, meta: u8) {
        let (pkt_src, pkt_dest) = return_if_none!(extract_meta(&mut buf));
        let socket = return_if_none!(self.sockets.get(&pkt_dest));
        if let Some((dest_node, dest_port)) = socket.target {
            if dest_node != from_node {
                log::warn!("[SocketFeature] Recv failed, node mismatch: {} != {}", dest_node, from_node);
                return;
            }
            if dest_port != pkt_dest {
                log::warn!("[SocketFeature] Recv failed, port mismatch: {} != {}", dest_port, pkt_dest);
                return;
            }
            self.queue.push_back(FeatureWorkerOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, buf, meta)));
        } else {
            self.queue.push_back(FeatureWorkerOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, buf, meta)))
        }
    }
}

impl<UserData> Default for SocketFeatureWorker<UserData> {
    fn default() -> Self {
        Self {
            sockets: HashMap::new(),
            queue: Default::default(),
        }
    }
}

impl<UserData: Clone + Copy + Eq> FeatureWorker<UserData, Control, Event, ToController, ToWorker<UserData>> for SocketFeatureWorker<UserData> {
    fn on_input(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<UserData, Control, ToWorker<UserData>>) {
        match input {
            FeatureWorkerInput::Network(_conn, meta, buf) => {
                let from_node = return_if_none!(meta.source);
                self.process_incoming(from_node, buf, meta.meta);
            }
            FeatureWorkerInput::FromController(_, control) => match control {
                ToWorker::BindSocket(port, actor) => {
                    log::info!("[SocketFeatureWorker] BindSocket: {port}");
                    self.sockets.insert(port, Socket { target: None, actor });
                }
                ToWorker::ConnectSocket(port, dest_node, dest_port) => {
                    log::info!("[SocketFeatureWorker] ConnectSocket: {port} => {dest_node}:{dest_port}");
                    if let Some(socket) = self.sockets.get_mut(&port) {
                        socket.target = Some((dest_node, dest_port));
                    }
                }
                ToWorker::UnbindSocket(port) => {
                    log::info!("[SocketFeatureWorker] UnbindSocket: {port}");
                    self.sockets.remove(&port);
                }
            },
            FeatureWorkerInput::Control(actor, control) => {
                let (port, (dest_node, dest_port), mut data, meta) = match control {
                    Control::Send(port, data, meta) => {
                        let socket = return_if_none!(self.sockets.get(&port));
                        if actor == socket.actor {
                            let target = return_if_none!(socket.target);
                            (port, target, data, meta)
                        } else {
                            return;
                        }
                    }
                    Control::SendTo(port, dest_node, dest_port, data, meta) => {
                        let socket = return_if_none!(self.sockets.get(&port));
                        if actor == socket.actor {
                            (port, (dest_node, dest_port), data, meta)
                        } else {
                            return;
                        }
                    }
                    _ => {
                        self.queue.push_back(FeatureWorkerOutput::ForwardControlToController(actor, control));
                        return;
                    }
                };

                embed_meta(port, dest_port, &mut data);
                let outgoing_meta = NetOutgoingMeta::new(true, Ttl::default(), meta, false);
                self.queue.push_back(FeatureWorkerOutput::SendRoute(RouteRule::ToNode(dest_node), outgoing_meta, data));
            }
            FeatureWorkerInput::Local(meta, buf) => {
                let from_node = return_if_none!(meta.source);
                self.process_incoming(from_node, buf, meta.meta);
            }
            _ => {}
        }
    }
}

impl<UserData> TaskSwitcherChild<WorkerOutput<UserData>> for SocketFeatureWorker<UserData> {
    type Time = u64;
    fn pop_output(&mut self, _now: u64) -> Option<WorkerOutput<UserData>> {
        self.queue.pop_front()
    }
}

fn embed_meta(src: u16, dest: u16, data: &mut Buffer) {
    data.ensure_front(4);
    data.push_front(&dest.to_be_bytes());
    data.push_front(&src.to_be_bytes());
}

fn extract_meta(buf: &mut Buffer) -> Option<(u16, u16)> {
    let src_buf2 = buf.pop_front(2)?;
    let src_buf = src_buf2.deref();
    let src = u16::from_be_bytes([src_buf[0], src_buf[1]]);

    let dest_buf2 = buf.pop_front(2)?;
    let dest_buf = dest_buf2.deref();
    let dest = u16::from_be_bytes([dest_buf[0], dest_buf[1]]);

    Some((src, dest))
}

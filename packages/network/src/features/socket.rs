use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;

use crate::base::{
    Feature, FeatureContext, FeatureControlActor, FeatureInput, FeatureOutput, FeatureSharedInput, FeatureWorker, FeatureWorkerContext, FeatureWorkerInput, FeatureWorkerOutput, GenericBuffer,
    NetOutgoingMeta, TransportMsgHeader, Ttl,
};

pub const FEATURE_ID: u8 = 7;
pub const FEATURE_NAME: &str = "socket";

const MTU_SIZE: usize = 1300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Bind(u16),
    Connect(u16, NodeId, u16),
    SendTo(u16, NodeId, u16, Vec<u8>, u8),
    Send(u16, Vec<u8>, u8),
    Unbind(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    RecvFrom(u16, NodeId, u16, Vec<u8>, u8),
}

#[derive(Debug, Clone)]
pub enum ToWorker {
    BindSocket(u16, FeatureControlActor),
    ConnectSocket(u16, NodeId, u16),
    UnbindSocket(u16),
}

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
    fn send_to(&mut self, src: u16, dest_node: NodeId, dest_port: u16, data: Vec<u8>, meta: u8) {
        if let Some(size) = serialize_msg(&mut self.temp, src, dest_port, &data) {
            let meta: NetOutgoingMeta = NetOutgoingMeta::new(true, Default::default(), meta);
            self.queue.push_back(FeatureOutput::SendRoute(RouteRule::ToNode(dest_node), meta, self.temp[..size].to_vec()));
        }
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
            FeatureInput::Net(_, meta, buf) | FeatureInput::Local(meta, buf) => {
                let from_node = if let Some(source) = meta.source {
                    source
                } else {
                    log::warn!("[SocketFeature] Recv failed, source not set");
                    return;
                };
                let (pkt_src, pkt_dest, data) = if let Some(res) = deserialize_msg(&buf) {
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
                        self.queue
                            .push_back(FeatureOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, data.to_vec(), meta.meta)));
                    } else {
                        self.queue
                            .push_back(FeatureOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, data.to_vec(), meta.meta)));
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

pub struct SocketFeatureWorker {
    sockets: HashMap<u16, Socket>,
    buf: [u8; MTU_SIZE],
}

impl SocketFeatureWorker {
    fn process_incoming(&self, from_node: NodeId, buf: &[u8], meta: u8) -> Option<FeatureWorkerOutput<'static, Control, Event, ToController>> {
        let (pkt_src, pkt_dest, data) = deserialize_msg(&buf)?;
        let socket = self.sockets.get(&pkt_dest)?;
        if let Some((dest_node, dest_port)) = socket.target {
            if dest_node != from_node {
                log::warn!("[SocketFeature] Recv failed, node mismatch: {} != {}", dest_node, from_node);
                return None;
            }
            if dest_port != pkt_dest {
                log::warn!("[SocketFeature] Recv failed, port mismatch: {} != {}", dest_port, pkt_dest);
                return None;
            }
            Some(FeatureWorkerOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, data.to_vec(), meta)))
        } else {
            Some(FeatureWorkerOutput::Event(socket.actor, Event::RecvFrom(pkt_dest, from_node, pkt_src, data.to_vec(), meta)))
        }
    }
}

impl Default for SocketFeatureWorker {
    fn default() -> Self {
        Self {
            sockets: HashMap::new(),
            buf: [0; MTU_SIZE],
        }
    }
}

impl FeatureWorker<Control, Event, ToController, ToWorker> for SocketFeatureWorker {
    fn on_network_raw<'a>(
        &mut self,
        _ctx: &mut FeatureWorkerContext,
        _now: u64,
        _conn: atm0s_sdn_identity::ConnId,
        _remote: std::net::SocketAddr,
        header: TransportMsgHeader,
        buf: GenericBuffer<'a>,
    ) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        self.process_incoming(header.from_node?, &(&buf)[header.serialize_size()..], header.meta)
    }

    fn on_input<'a>(&mut self, _ctx: &mut FeatureWorkerContext, _now: u64, input: FeatureWorkerInput<'a, Control, ToWorker>) -> Option<FeatureWorkerOutput<'a, Control, Event, ToController>> {
        match input {
            FeatureWorkerInput::FromController(_, control) => match control {
                ToWorker::BindSocket(port, actor) => {
                    log::info!("[SocketFeatureWorker] BindSocket: {port}");
                    self.sockets.insert(port, Socket { target: None, actor });
                    None
                }
                ToWorker::ConnectSocket(port, dest_node, dest_port) => {
                    log::info!("[SocketFeatureWorker] ConnectSocket: {port} => {dest_node}:{dest_port}");
                    if let Some(socket) = self.sockets.get_mut(&port) {
                        socket.target = Some((dest_node, dest_port));
                    }
                    None
                }
                ToWorker::UnbindSocket(port) => {
                    log::info!("[SocketFeatureWorker] UnbindSocket: {port}");
                    self.sockets.remove(&port);
                    None
                }
            },
            FeatureWorkerInput::Control(actor, control) => {
                let (port, (dest_node, dest_port), data, meta) = match control {
                    Control::Send(port, data, meta) => {
                        let socket = self.sockets.get(&port)?;
                        if actor == socket.actor {
                            (port, socket.target?, data, meta)
                        } else {
                            return None;
                        }
                    }
                    Control::SendTo(port, dest_node, dest_port, data, meta) => {
                        let socket = self.sockets.get(&port)?;
                        if actor == socket.actor {
                            (port, (dest_node, dest_port), data, meta)
                        } else {
                            return None;
                        }
                    }
                    _ => return Some(FeatureWorkerOutput::ForwardControlToController(actor, control)),
                };

                let size = serialize_msg(&mut self.buf, port, dest_port, &data)?;
                let outgoing_meta = NetOutgoingMeta::new(true, Ttl::default(), meta);
                Some(FeatureWorkerOutput::SendRoute(RouteRule::ToNode(dest_node), outgoing_meta, self.buf[0..size].to_vec()))
            }
            FeatureWorkerInput::Local(meta, buf) | FeatureWorkerInput::Network(_, meta, buf) => self.process_incoming(meta.source?, &buf, meta.meta),
            _ => None,
        }
    }
}

fn serialize_msg(buf: &mut [u8], src: u16, dest: u16, data: &[u8]) -> Option<usize> {
    if data.len() > buf.len() - 4 {
        return None;
    }
    buf[..2].copy_from_slice(&src.to_be_bytes());
    buf[2..4].copy_from_slice(&dest.to_be_bytes());
    buf[4..(4 + data.len())].copy_from_slice(&data);
    Some(data.len() + 4)
}

fn deserialize_msg<'a>(buf: &'a [u8]) -> Option<(u16, u16, &'a [u8])> {
    if buf.len() < 4 {
        log::debug!("[SocketFeature] Invalid message length: {}, min is 4 bytes", buf.len());
        return None;
    }
    let src = u16::from_be_bytes([buf[0], buf[1]]);
    let dest = u16::from_be_bytes([buf[2], buf[3]]);
    Some((src, dest, &buf[4..]))
}

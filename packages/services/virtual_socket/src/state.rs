use std::{collections::HashMap, sync::Arc};

use async_std::channel::{Receiver, Sender};
use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::transport::ConnectionEvent;
use atm0s_sdn_utils::{awaker::Awaker, error_handle::ErrorUtils, option_handle::OptionUtils, vec_dequeue::VecDeque};
use parking_lot::RwLock;

use crate::msg::{SocketId, VirtualSocketControlMsg};

use self::socket::{VirtualSocketBuilder, VirtualSocketEvent, CONTROL_CLIENT_META, CONTROL_SERVER_META, DATA_CLIENT_META, DATA_SERVER_META};

pub mod connector;
pub mod listener;
pub mod socket;
pub mod stream;

const CONNECT_TIMEOUT_MS: u64 = 10000;
const PING_INTERVAL_MS: u64 = 1000;
const PING_TIMEOUT_MS: u64 = 10000;

enum OutgoingState {
    Connecting { started_at: u64, res_tx: Sender<VirtualSocketConnectResult> },
    Connected { last_ping: u64, pong_time: u64, tx: Sender<Vec<u8>> },
}

struct IncommingState {
    ping_time: u64,
    tx: Sender<Vec<u8>>,
}

pub enum VirtualSocketConnectResult {
    Success(VirtualSocketBuilder),
    Timeout,
    Unreachable,
}

pub struct State {
    client_idx: u32,
    last_tick_ms: u64,
    listeners: HashMap<String, Sender<VirtualSocketBuilder>>,
    outgoings: HashMap<SocketId, OutgoingState>,
    incomings: HashMap<SocketId, IncommingState>,
    outgoing_queue: VecDeque<(SocketId, VirtualSocketEvent)>,
    awaker: Option<Arc<dyn Awaker>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            client_idx: 0,
            last_tick_ms: 0,
            listeners: HashMap::new(),
            outgoings: HashMap::new(),
            incomings: HashMap::new(),
            outgoing_queue: VecDeque::new(),
            awaker: None,
        }
    }
}

impl State {
    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.awaker = Some(awaker);
    }

    pub fn new_listener(&mut self, id: &str) -> Receiver<VirtualSocketBuilder> {
        log::info!("[VirtualSocketState] new listener: {}", id);
        let (tx, rx) = async_std::channel::bounded(10);
        self.listeners.insert(id.to_string(), tx);
        rx
    }

    pub fn new_outgoing(&mut self, dest_node_id: NodeId, dest_listener_id: &str, meta: HashMap<String, String>) -> Option<Receiver<VirtualSocketConnectResult>> {
        let client_idx = self.client_idx;
        self.client_idx += 1;
        log::info!("[VirtualSocketState] new outgoing: {}/{} with meta {:?} => idx {}", dest_node_id, dest_listener_id, meta, client_idx);
        let socket_id = SocketId(dest_node_id, client_idx);

        let (tx, rx) = async_std::channel::bounded(1);
        self.outgoings.insert(
            socket_id.clone(),
            OutgoingState::Connecting {
                started_at: self.last_tick_ms,
                res_tx: tx,
            },
        );
        self.outgoing_queue.push_back((
            socket_id,
            VirtualSocketEvent::ClientControl(VirtualSocketControlMsg::ConnectRequest(dest_listener_id.to_string(), meta)),
        ));
        Some(rx)
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.last_tick_ms = now_ms;

        // Remove timed out outgoing connections
        let mut to_remove = Vec::new();
        for (socket_id, state) in self.outgoings.iter() {
            if let OutgoingState::Connecting { started_at, res_tx: tx } = state {
                if now_ms - started_at > CONNECT_TIMEOUT_MS {
                    log::info!("[VirtualSocketState] outgoing timeout: node {} idx: {}", socket_id.node_id(), socket_id.client_id());
                    to_remove.push(socket_id.clone());
                    tx.try_send(VirtualSocketConnectResult::Timeout).print_error("Should send timeout to waiting connector");
                }
            }
        }
        for socket_id in to_remove {
            self.outgoings.remove(&socket_id).print_none("Should remove timed out outgoing connection");
        }

        // send ping from outgoing sockets
        for (socket_id, state) in self.outgoings.iter_mut() {
            if let OutgoingState::Connected { last_ping, .. } = state {
                if now_ms - *last_ping > PING_INTERVAL_MS {
                    *last_ping = now_ms;
                    self.outgoing_queue
                        .push_back((socket_id.clone(), VirtualSocketEvent::ClientControl(VirtualSocketControlMsg::ConnectingPing)));
                }
            }
        }

        // Remote ping timeout outgoing sockets
        let mut to_remove = Vec::new();
        for (socket_id, state) in self.outgoings.iter() {
            if let OutgoingState::Connected { pong_time, .. } = state {
                if now_ms - *pong_time > PING_TIMEOUT_MS {
                    log::info!("[VirtualSocketState] outgoing ping timeout: node {} idx: {}", socket_id.node_id(), socket_id.client_id());
                    to_remove.push(socket_id.clone());
                }
            }
        }
        for socket_id in to_remove {
            self.outgoings.remove(&socket_id).print_none("Should remove timed out outgoing connection");
        }

        // Remove timed out incoming sockets
        let mut to_remove = Vec::new();
        for (socket_id, state) in self.incomings.iter() {
            if now_ms - state.ping_time > PING_TIMEOUT_MS {
                log::info!("[VirtualSocketState] incoming ping timeout: node {} idx: {}", socket_id.node_id(), socket_id.client_id());
                to_remove.push(socket_id.clone());
            }
        }
        for socket_id in to_remove {
            self.incomings.remove(&socket_id).print_none("Should remove timed out incoming connection");
        }
    }

    pub fn on_recv_server_data(&self, _now_ms: u64, socket_id: SocketId, data: &[u8]) {
        log::debug!("[VirtualSocketState] on_recv_server_data: {:?} {:?}", socket_id, data);
        if let Some(OutgoingState::Connected { tx, .. }) = self.outgoings.get(&socket_id) {
            tx.try_send(data.to_vec()).ok();
        }
    }

    pub fn on_recv_client_data(&self, _now_ms: u64, socket_id: SocketId, data: &[u8]) {
        log::debug!("[VirtualSocketState] on_recv_client_data: {:?} {:?}", socket_id, data);
        if let Some(state) = self.incomings.get(&socket_id) {
            state.tx.try_send(data.to_vec()).ok();
        }
    }

    pub fn send_out(&mut self, socket_id: SocketId, event: VirtualSocketEvent) {
        log::debug!("[VirtualSocketState] send_out to : {:?} {:?}", socket_id, event);
        self.outgoing_queue.push_back((socket_id, event));
        if self.outgoing_queue.len() == 1 {
            if let Some(awaker) = self.awaker.as_ref() {
                awaker.notify();
            }
        }
    }

    pub fn on_recv_server_control(&mut self, now_ms: u64, socket_id: SocketId, control: VirtualSocketControlMsg) {
        log::debug!("[VirtualSocketState] on_recv_server_control from : {:?} {:?}", socket_id, control);
        match control {
            VirtualSocketControlMsg::ConnectReponse(success) => {
                if let Some(state) = self.outgoings.get_mut(&socket_id) {
                    if let OutgoingState::Connecting { res_tx, .. } = state {
                        if success {
                            let (socket_tx, socket_rx) = async_std::channel::bounded(10);
                            res_tx
                                .try_send(VirtualSocketConnectResult::Success(VirtualSocketBuilder::new(true, socket_id, HashMap::new(), socket_rx)))
                                .print_error("Should send connect response to waiting connector");
                            *state = OutgoingState::Connected {
                                last_ping: now_ms,
                                pong_time: now_ms,
                                tx: socket_tx,
                            };
                        } else {
                            res_tx
                                .try_send(VirtualSocketConnectResult::Unreachable)
                                .print_error("Should send connect response to waiting connector");
                            self.outgoings.remove(&socket_id).print_none("Should remove failed outgoing connection");
                        };
                    } else {
                        log::warn!("[VirtualSocketState] on_recv_server_control socket already connected: {:?}", socket_id);
                    }
                } else {
                    log::warn!("[VirtualSocketState] on_recv_server_control socket not found: {:?}", socket_id);
                }
            }
            VirtualSocketControlMsg::ConnectionClose() => {
                if self.incomings.remove(&socket_id).is_some() {
                    log::info!("[VirtualSocketState] closed outgoing socket: {:?}", socket_id);
                }
            }
            VirtualSocketControlMsg::ConnectingPong => {
                //update pong time to outgoing sockets
                if let Some(state) = self.outgoings.get_mut(&socket_id) {
                    if let OutgoingState::Connected { pong_time, .. } = state {
                        *pong_time = now_ms;
                    }
                }
            }
            _ => {
                log::warn!("[VirtualSocketState] on_recv_server_control Unknown control message: {:?}", control);
            }
        }
    }

    pub fn on_recv_client_control(&mut self, now_ms: u64, socket_id: SocketId, control: VirtualSocketControlMsg) {
        log::debug!("[VirtualSocketState] on_recv_client_control from : {:?} {:?}", socket_id, control);
        match control {
            VirtualSocketControlMsg::ConnectRequest(listener_id, meta) => {
                if self.incomings.contains_key(&socket_id) {
                    log::warn!("[VirtualSocketState] on_recv_client_control socket already connected: {:?}", socket_id);
                    return;
                }
                if let Some(tx) = self.listeners.get(&listener_id) {
                    let (socket_tx, socket_rx) = async_std::channel::bounded(10);
                    self.incomings.insert(socket_id.clone(), IncommingState { ping_time: now_ms, tx: socket_tx });
                    tx.try_send(VirtualSocketBuilder::new(false, socket_id.clone(), meta, socket_rx))
                        .print_error("Should send new virtual socket to listener");
                    self.outgoing_queue
                        .push_back((socket_id, VirtualSocketEvent::ServerControl(VirtualSocketControlMsg::ConnectReponse(true))));
                } else {
                    self.outgoing_queue
                        .push_back((socket_id, VirtualSocketEvent::ServerControl(VirtualSocketControlMsg::ConnectReponse(false))));
                }
            }
            VirtualSocketControlMsg::ConnectionClose() => {
                if self.incomings.remove(&socket_id).is_some() {
                    log::info!("[VirtualSocketState] closed incoming socket: {:?}", socket_id);
                }
            }
            VirtualSocketControlMsg::ConnectingPing => {
                //update ping time to incoming sockets
                if let Some(state) = self.incomings.get_mut(&socket_id) {
                    state.ping_time = now_ms;
                }
                self.outgoing_queue.push_back((socket_id, VirtualSocketEvent::ServerControl(VirtualSocketControlMsg::ConnectingPong)));
            }
            _ => {
                log::warn!("[VirtualSocketState] on_recv_client_control Unknown control message: {:?}", control);
            }
        }
    }

    pub fn close_socket(&mut self, is_client: bool, socket_id: &SocketId) {
        if is_client {
            if self.outgoings.remove(socket_id).is_some() {
                log::debug!("[VirtualSocketState] will close outgoing socket: {:?}", socket_id);
                let socket_id: SocketId = socket_id.clone();
                self.outgoing_queue
                    .push_back((socket_id, VirtualSocketEvent::ClientControl(VirtualSocketControlMsg::ConnectionClose())));
            }
        } else {
            if self.incomings.remove(socket_id).is_some() {
                log::debug!("[VirtualSocketState] will close incomming socket: {:?}", socket_id);
                let socket_id: SocketId = socket_id.clone();
                self.outgoing_queue
                    .push_back((socket_id, VirtualSocketEvent::ServerControl(VirtualSocketControlMsg::ConnectionClose())));
            }
        }
    }

    pub fn pop_outgoing(&mut self) -> Option<(SocketId, VirtualSocketEvent)> {
        self.outgoing_queue.pop_front()
    }
}

pub fn process_incoming_data(now_ms: u64, state: &RwLock<State>, event: ConnectionEvent) {
    if let ConnectionEvent::Msg(data) = event {
        if let Some(from) = data.header.from_node {
            match data.header.meta {
                DATA_CLIENT_META => {
                    let socket_id = SocketId(from, data.header.stream_id);
                    state.read().on_recv_client_data(now_ms, socket_id, data.payload());
                }
                DATA_SERVER_META => {
                    let socket_id = SocketId(from, data.header.stream_id);
                    state.read().on_recv_server_data(now_ms, socket_id, data.payload());
                }
                CONTROL_CLIENT_META => {
                    if let Ok(control) = data.get_payload_bincode::<VirtualSocketControlMsg>() {
                        //is control
                        if let Some(from) = data.header.from_node {
                            state.write().on_recv_client_control(now_ms, SocketId(from, data.header.stream_id), control);
                        }
                    }
                }
                CONTROL_SERVER_META => {
                    if let Ok(control) = data.get_payload_bincode::<VirtualSocketControlMsg>() {
                        //is control
                        if let Some(from) = data.header.from_node {
                            state.write().on_recv_server_control(now_ms, SocketId(from, data.header.stream_id), control);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {}

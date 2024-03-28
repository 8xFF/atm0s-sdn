use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId, Protocol};

use crate::base::{self, Authorization, ConnectionCtx, HandshakeBuilder, NeighboursControl, NeighboursControlCmds, SecureContext};

use self::connection::{ConnectionEvent, NeighbourConnection};

mod connection;

pub enum Input {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    Control(SocketAddr, NeighboursControl),
    ShutdownRequest,
}

pub enum Output {
    Control(SocketAddr, NeighboursControl),
    Event(base::ConnectionEvent),
    ShutdownResponse,
}

pub struct NeighboursManager {
    node_id: NodeId,
    connections: HashMap<SocketAddr, NeighbourConnection>,
    neighbours: HashMap<ConnId, ConnectionCtx>,
    queue: VecDeque<Output>,
    shutdown: bool,
    authorization: Arc<dyn Authorization>,
    handshake_builder: Arc<dyn HandshakeBuilder>,
    random: Box<dyn rand::RngCore>,
}

impl NeighboursManager {
    pub fn new(node_id: NodeId, authorization: Arc<dyn Authorization>, handshake_builder: Arc<dyn HandshakeBuilder>, random: Box<dyn rand::RngCore>) -> Self {
        Self {
            node_id,
            connections: HashMap::new(),
            neighbours: HashMap::new(),
            queue: VecDeque::new(),
            shutdown: false,
            authorization,
            handshake_builder,
            random,
        }
    }

    pub fn conn(&self, conn: ConnId) -> Option<&ConnectionCtx> {
        self.neighbours.get(&conn)
    }

    pub fn on_tick(&mut self, now_ms: u64, _tick_count: u64) {
        for conn in self.connections.values_mut() {
            conn.on_tick(now_ms);
        }
    }

    pub fn on_input(&mut self, now_ms: u64, input: Input) {
        match input {
            Input::ConnectTo(addr) => {
                let dest_node = addr.node_id();
                let dests = get_node_addr_dests(addr);
                for remote in dests {
                    if self.connections.contains_key(&remote) {
                        continue;
                    }
                    log::info!("[Neighbours] Sending connect request to {}, dest_node {}", remote, dest_node);
                    let session_id = self.random.next_u64();
                    let conn = NeighbourConnection::new_outgoing(self.handshake_builder.clone(), self.node_id, dest_node, session_id, remote, now_ms);
                    self.connections.insert(remote, conn);
                }
            }
            Input::DisconnectFrom(node) => {
                for conn in self.connections.values_mut() {
                    if conn.dest_node() == node {
                        conn.disconnect(now_ms);
                    }
                }
            }
            Input::Control(addr, control) => {
                let cmd: NeighboursControlCmds = match control.validate(now_ms, &*self.authorization) {
                    Ok(cmd) => cmd,
                    Err(_) => {
                        log::warn!("[Neighbours] Invalid control from {:?}", addr);
                        return;
                    }
                };

                log::debug!("[NeighboursManager] received Control(addr: {:?}, control: {:?})", addr, control);
                if let Some(conn) = self.connections.get_mut(&addr) {
                    conn.on_input(now_ms, control.from, cmd);
                } else {
                    match cmd {
                        NeighboursControlCmds::ConnectRequest { session, .. } => {
                            let mut conn = NeighbourConnection::new_incoming(self.handshake_builder.clone(), self.node_id, control.from, session, addr, now_ms);
                            conn.on_input(now_ms, control.from, cmd);
                            self.connections.insert(addr, conn);
                        }
                        _ => {
                            log::warn!("[Neighbours] Neighbour connection not found for control {:?}", control);
                        }
                    }
                }
            }
            Input::ShutdownRequest => {
                self.shutdown = true;
                for conn in self.connections.values_mut() {
                    conn.disconnect(now_ms);
                }
                if self.connections.is_empty() {
                    self.queue.push_back(Output::ShutdownResponse);
                }
            }
        }
    }

    pub fn pop_output(&mut self, now_ms: u64) -> Option<Output> {
        if let Some(output) = self.queue.pop_front() {
            return Some(output);
        }

        let mut to_remove = Vec::new();
        for (remote, conn) in self.connections.iter_mut() {
            while let Some(output) = conn.pop_output() {
                match output {
                    connection::Output::Event(event) => {
                        let event = match event {
                            ConnectionEvent::Connected(encryptor, decryptor) => {
                                let ctx = conn.ctx();
                                self.neighbours.insert(ctx.conn, ctx.clone());
                                Some(base::ConnectionEvent::Connected(ctx, SecureContext { encryptor, decryptor }))
                            }
                            ConnectionEvent::ConnectError(_) => {
                                to_remove.push(*remote);
                                None
                            }
                            ConnectionEvent::ConnectTimeout => {
                                to_remove.push(*remote);
                                None
                            }
                            ConnectionEvent::Stats(stats) => {
                                let ctx = conn.ctx();
                                Some(base::ConnectionEvent::Stats(ctx, stats))
                            }
                            ConnectionEvent::Disconnected => {
                                let ctx = conn.ctx();
                                self.neighbours.remove(&ctx.conn);
                                to_remove.push(*remote);
                                Some(base::ConnectionEvent::Disconnected(ctx))
                            }
                        };
                        if let Some(event) = event {
                            self.queue.push_back(Output::Event(event));
                        }
                    }
                    connection::Output::Net(remote, cmd) => {
                        log::debug!("[NeighboursManager] pop_output Net(remote: {:?}, cmd: {:?})", remote, cmd);
                        self.queue.push_back(Output::Control(remote, NeighboursControl::build(now_ms, self.node_id, cmd, &*self.authorization)));
                    }
                }
            }
        }

        for remote in to_remove {
            self.connections.remove(&remote);

            if self.shutdown && self.connections.is_empty() {
                self.queue.push_back(Output::ShutdownResponse);
            }
        }

        self.queue.pop_front()
    }
}

fn get_node_addr_dests(addr: NodeAddr) -> Vec<SocketAddr> {
    let mut dests = Vec::new();
    log::info!("Connect to: addr {}", addr);
    let mut dest_ip = None;
    for part in addr.multiaddr().iter() {
        match part {
            Protocol::Ip4(i) => {
                dest_ip = Some(IpAddr::V4(i));
            }
            Protocol::Ip6(i) => {
                dest_ip = Some(IpAddr::V6(i));
            }
            Protocol::Udp(port) => {
                if let Some(ip) = dest_ip {
                    dests.push(SocketAddr::new(ip, port));
                }
            }
            _ => {}
        }
    }
    dests
}

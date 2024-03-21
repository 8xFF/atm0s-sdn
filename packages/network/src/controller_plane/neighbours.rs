use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId, Protocol};

use crate::base::{self, ConnectionCtx, NeighboursControl, NeighboursControlCmds, SecureContext};

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
    random: Box<dyn rand::RngCore>,
}

impl NeighboursManager {
    pub fn new(node_id: NodeId, random: Box<dyn rand::RngCore>) -> Self {
        Self {
            node_id,
            connections: HashMap::new(),
            neighbours: HashMap::new(),
            queue: VecDeque::new(),
            shutdown: false,
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
                //TODO move to other
                let dest_node = addr.node_id();
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
                                let remote = SocketAddr::new(ip, port);
                                log::info!("Sending connect request to {}, dest_node {}", remote, dest_node);
                                let session_id = self.random.next_u64();
                                let conn = NeighbourConnection::new_outgoing(self.node_id, dest_node, session_id, remote, now_ms);
                                self.connections.insert(remote, conn);
                            }
                        }
                        _ => {}
                    }
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
                if let Some(conn) = self.connections.get_mut(&addr) {
                    conn.on_input(now_ms, control);
                } else {
                    match &control.cmd {
                        NeighboursControlCmds::ConnectRequest { from, to: _, session } => {
                            let mut conn = NeighbourConnection::new_incoming(self.node_id, *from, *session, addr, now_ms);
                            conn.on_input(now_ms, control);
                            self.connections.insert(addr, conn);
                        }
                        _ => {
                            log::warn!("Neighbour connection not found for control {:?}", control);
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

    pub fn pop_output(&mut self) -> Option<Output> {
        if let Some(output) = self.queue.pop_front() {
            return Some(output);
        }

        let mut to_remove = Vec::new();
        for (remote, conn) in self.connections.iter_mut() {
            while let Some(output) = conn.pop_output() {
                match output {
                    connection::Output::Event(event) => {
                        let event = match event {
                            ConnectionEvent::Connected => {
                                let ctx = conn.ctx();
                                self.neighbours.insert(ctx.conn, ctx.clone());
                                Some(base::ConnectionEvent::Connected(ctx, SecureContext {}))
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
                    connection::Output::Net(remote, control) => {
                        self.queue.push_back(Output::Control(remote, control));
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

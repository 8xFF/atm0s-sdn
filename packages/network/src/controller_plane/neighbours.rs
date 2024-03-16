use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
};

use atm0s_sdn_identity::{ConnDirection, ConnId, NodeAddr, NodeId, Protocol};
use atm0s_sdn_utils::random::{self, Random};

use crate::base::{self, ConnectionCtx, NeigboursControlCmds, NeighboursControl};

use self::connection::{ConnectionEvent, NeighbourConnection};

mod connection;

pub enum Input {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    Control(SocketAddr, NeighboursControl),
}

pub enum Output {
    Control(SocketAddr, NeighboursControl),
    Event(base::ConnectionEvent),
}

pub struct NeighboursManager {
    conn_id_seed: u64,
    node_id: NodeId,
    connections: HashMap<SocketAddr, NeighbourConnection>,
    neighbours: HashMap<ConnId, ConnectionCtx>,
    queue: VecDeque<Output>,
}

impl NeighboursManager {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            conn_id_seed: 0,
            node_id,
            connections: HashMap::new(),
            neighbours: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn conn(&self, conn: ConnId) -> Option<&ConnectionCtx> {
        self.neighbours.get(&conn)
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        for conn in self.connections.values_mut() {
            conn.on_tick(now_ms);
        }
    }

    pub fn on_input(&mut self, now_ms: u64, input: Input) {
        match input {
            Input::ConnectTo(addr) => {
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
                                let conn_id = self.gen_conn_id(ConnDirection::Outgoing);
                                let session_id = Self::gen_session_id();
                                let conn = NeighbourConnection::new_outgoing(conn_id, self.node_id, dest_node, session_id, remote, now_ms);
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
                        NeigboursControlCmds::ConnectRequest { from, to: _, session } => {
                            let conn_id = self.gen_conn_id(ConnDirection::Incoming);
                            let conn = NeighbourConnection::new_incoming(conn_id, self.node_id, *from, *session, addr, now_ms);
                            self.connections.insert(addr, conn);
                        }
                        _ => {
                            log::warn!("Neighbour connection not found for control {:?}", control);
                        }
                    }
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
                                Some(base::ConnectionEvent::Connected(ctx))
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
        }

        self.queue.pop_front()
    }

    fn gen_conn_id(&mut self, direction: ConnDirection) -> ConnId {
        self.conn_id_seed += 1;
        ConnId::from_raw(0, direction, self.conn_id_seed)
    }

    /// Generates a random session ID.
    fn gen_session_id() -> u64 {
        random::RealRandom().random()
    }
}

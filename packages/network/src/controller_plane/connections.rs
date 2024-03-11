use std::{
    collections::{HashMap, VecDeque},
    net::{IpAddr, SocketAddr},
};

use atm0s_sdn_identity::{ConnDirection, ConnId, NodeAddr, NodeId, Protocol};
use serde::{Deserialize, Serialize};

const RESEND_CONNECT_MS: u64 = 500;
const TIMEOUT_CONNECT_MS: u64 = 5000;
const TIMEOUT_PING_MS: u64 = 5000;
const DEFAULT_TTL: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisconnectReason {
    RemoteDisconnect,
    Shutdown,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectError {
    WrongDestination,
    AlreadyConnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisconnectError {
    NotConnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlMsg {
    ConnectRequest { from: NodeId, to: NodeId },
    ConnectResponse(Result<(), ConnectError>),
    Ping(u16, u64),
    Pong(u16, u64),
    DisconnectRequest(DisconnectReason),
    DisconnectResponse(Result<(), DisconnectError>),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ConnectionStats {
    pub rtt: u32,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConnectionEvent {
    ConnectionEstablished(ConnId),
    ConnectionDisconnected(ConnId),
    ConnectionStats(ConnId, ConnectionStats),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Input {
    ConnectTo(NodeAddr),
    DisconnectFrom(NodeId),
    ControlIn(SocketAddr, ControlMsg),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Output {
    ControlOut(SocketAddr, ControlMsg),
    ConnectionEvent(ConnectionEvent),
}

struct Outgoing {
    to: NodeId,
    created_at: u64,
    last_sent: u64,
}

struct Connnection {
    node: NodeId,
    id: ConnId,
    last_pong: u64,
    ttl: u32,
    disconnect: Option<DisconnectReason>,
}

pub struct Connections {
    ping_seq: u16,
    conn_seed: u64,
    node_id: NodeId,
    outgoings: HashMap<SocketAddr, Outgoing>,
    conns: HashMap<SocketAddr, Connnection>,
    map: HashMap<ConnId, SocketAddr>,
    queue: VecDeque<Output>,
}

impl Connections {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            ping_seq: 0,
            conn_seed: 0,
            node_id,
            outgoings: HashMap::new(),
            conns: HashMap::new(),
            map: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        let mut timeout_outgoings = vec![];
        for (remote, outgoing) in self.outgoings.iter_mut() {
            if outgoing.created_at + TIMEOUT_CONNECT_MS <= now_ms {
                log::warn!("Connection to {} timed out", remote);
                timeout_outgoings.push(*remote);
            } else if outgoing.last_sent + RESEND_CONNECT_MS <= now_ms {
                self.queue.push_back(Output::ControlOut(*remote, ControlMsg::ConnectRequest { from: self.node_id, to: outgoing.to }));
                outgoing.last_sent = now_ms;
                log::warn!("Resending connect request to {}", remote)
            }
        }
        for remote in timeout_outgoings {
            self.outgoings.remove(&remote);
        }

        //sending pings
        let mut timeout_connections = vec![];
        for (remote, conn) in self.conns.iter() {
            if conn.last_pong + TIMEOUT_PING_MS <= now_ms {
                log::warn!("Connection to {}/{}/{} timed out", conn.node, conn.id, remote);
                self.queue.push_back(Output::ConnectionEvent(ConnectionEvent::ConnectionDisconnected(conn.id)));
                timeout_connections.push(*remote);
            } else if let Some(reason) = conn.disconnect {
                log::debug!("Resending disconnect to node {}, conn:{}, remote: {}", conn.node, conn.id, remote);
                self.queue.push_back(Output::ControlOut(*remote, ControlMsg::DisconnectRequest(reason)));
            } else {
                log::debug!("Sending ping to node {}, conn:{}, remote: {}", conn.node, conn.id, remote);
                self.queue.push_back(Output::ControlOut(*remote, ControlMsg::Ping(self.ping_seq, now_ms)));
            }
        }
        self.ping_seq += 1;
        for remote in timeout_connections {
            let conn = self.conns.remove(&remote).expect("Should have");
            self.map.remove(&conn.id);
        }
    }

    pub fn on_event(&mut self, now_ms: u64, event: Input) {
        match event {
            Input::ConnectTo(addr) => self.connect_to(now_ms, addr),
            Input::DisconnectFrom(node) => self.disconnect_from(node),
            Input::ControlIn(remote, msg) => match msg {
                ControlMsg::ConnectRequest { from, to } => {
                    if to == self.node_id {
                        if self.conns.contains_key(&remote) {
                            log::warn!("Received connect request from already established remote {} => reject", remote);
                            self.queue.push_back(Output::ControlOut(remote, ControlMsg::ConnectResponse(Err(ConnectError::AlreadyConnected))));
                            return;
                        }
                        self.queue.push_back(Output::ControlOut(remote, ControlMsg::ConnectResponse(Ok(()))));
                        let conn_id = self.generate_conn_id(ConnDirection::Incoming);
                        self.conns.insert(
                            remote,
                            Connnection {
                                node: from,
                                id: conn_id,
                                last_pong: now_ms,
                                ttl: DEFAULT_TTL,
                                disconnect: None,
                            },
                        );
                        self.map.insert(conn_id, remote);
                        self.queue.push_back(Output::ConnectionEvent(ConnectionEvent::ConnectionEstablished(conn_id)));
                        log::info!("Received connect request from {} => created connection with conn: {} remote {}", from, conn_id, remote);
                    } else {
                        log::warn!("Received connect request from {} with wrong destination: {} vs self {}", from, to, self.node_id);
                        self.queue.push_back(Output::ControlOut(remote, ControlMsg::ConnectResponse(Err(ConnectError::WrongDestination))));
                    }
                }
                ControlMsg::ConnectResponse(Ok(())) => {
                    if let Some(outgoing) = self.outgoings.remove(&remote) {
                        let conn_id = self.generate_conn_id(ConnDirection::Outgoing);
                        self.conns.insert(
                            remote,
                            Connnection {
                                node: outgoing.to,
                                id: conn_id,
                                last_pong: now_ms,
                                ttl: DEFAULT_TTL,
                                disconnect: None,
                            },
                        );
                        self.map.insert(conn_id, remote);
                        self.queue.push_back(Output::ConnectionEvent(ConnectionEvent::ConnectionEstablished(conn_id)));
                        log::info!("Connected to remote {}, conn: {}, node: {}", remote, conn_id, outgoing.to);
                    } else {
                        log::warn!("Received connect response from unknown remote {}", remote);
                    }
                }
                ControlMsg::ConnectResponse(Err(err)) => {
                    if let Some(outgoing) = self.outgoings.remove(&remote) {
                        log::warn!("Failed to connect to remote {}, node: {}, err {:?}", remote, outgoing.to, err);
                    } else {
                        log::warn!("Received connect response from unknown remote {}", remote);
                    }
                }
                ControlMsg::DisconnectRequest(reason) => {
                    if let Some(conn) = self.conns.remove(&remote) {
                        self.map.remove(&conn.id);
                        log::info!("Received disconnect request from {} with conn {}, reason {:?}", remote, conn.id, reason);
                        self.queue.push_back(Output::ConnectionEvent(ConnectionEvent::ConnectionDisconnected(conn.id)));
                        self.queue.push_back(Output::ControlOut(remote, ControlMsg::DisconnectResponse(Ok(()))));
                    } else {
                        log::warn!("Received disconnect request from unknown remote {}, reason {:?}", remote, reason);
                        self.queue.push_back(Output::ControlOut(remote, ControlMsg::DisconnectResponse(Err(DisconnectError::NotConnected))));
                    }
                }
                ControlMsg::DisconnectResponse(Ok(())) => {
                    if let Some(conn) = self.conns.get(&remote) {
                        if conn.disconnect.is_some() {
                            self.map.remove(&conn.id);
                            log::info!("Received disconnect response from {} with conn {}", remote, conn.id);
                            self.queue.push_back(Output::ConnectionEvent(ConnectionEvent::ConnectionDisconnected(conn.id)));
                            self.conns.remove(&remote);
                        } else {
                            log::warn!("Received disconnect response from {} with conn {} but not disconnecting", remote, conn.id);
                        }
                    } else {
                        log::warn!("Received disconnect response from unknown remote {}", remote);
                    }
                }
                ControlMsg::DisconnectResponse(Err(err)) => {
                    if let Some(conn) = self.conns.get(&remote) {
                        if conn.disconnect.is_some() {
                            self.map.remove(&conn.id);
                            log::info!("Received disconnect response error {:?} from {} with conn {} => force close", err, remote, conn.id);
                            self.queue.push_back(Output::ConnectionEvent(ConnectionEvent::ConnectionDisconnected(conn.id)));
                            self.conns.remove(&remote);
                        } else {
                            log::warn!("Received disconnect response from {} with conn {} but not disconnecting", remote, conn.id);
                        }
                    } else {
                        log::warn!("Received disconnect response from unknown remote {}", remote);
                    }
                }
                ControlMsg::Ping(seq, timestamp) => {
                    self.queue.push_back(Output::ControlOut(remote, ControlMsg::Pong(seq, timestamp)));
                }
                ControlMsg::Pong(_seq, timestamp) => {
                    if let Some(conn) = self.conns.get_mut(&remote) {
                        conn.ttl = (now_ms - timestamp) as u32;
                        conn.last_pong = now_ms;
                        log::debug!("Received pong from remote: {}, conn: {}, ttl: {}", remote, conn.id, conn.ttl);
                        self.queue
                            .push_back(Output::ConnectionEvent(ConnectionEvent::ConnectionStats(conn.id, ConnectionStats { rtt: conn.ttl as u32 })));
                    }
                }
            },
        }
    }

    pub fn pop_output(&mut self) -> Option<Output> {
        self.queue.pop_front()
    }

    /// Connect to a remote node. If the address contains multiple addresses, multiple connections will be attempted.
    fn connect_to(&mut self, now_ms: u64, addr: NodeAddr) {
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
                        self.outgoings.insert(
                            remote,
                            Outgoing {
                                to: dest_node,
                                created_at: now_ms,
                                last_sent: now_ms,
                            },
                        );
                        log::info!("Sending connect request to {}, dest_node {}", remote, dest_node);
                        self.queue.push_back(Output::ControlOut(remote, ControlMsg::ConnectRequest { from: self.node_id, to: dest_node }));
                    }
                }
                _ => {}
            }
        }
    }

    fn disconnect_from(&mut self, node: NodeId) {
        log::info!("Disconnect_from: {}", node);

        for (remote, conn) in self.conns.iter_mut() {
            if conn.node == node {
                conn.disconnect = Some(DisconnectReason::RemoteDisconnect);
                self.queue.push_back(Output::ControlOut(*remote, ControlMsg::DisconnectRequest(DisconnectReason::RemoteDisconnect)));
            }
        }
    }

    fn generate_conn_id(&mut self, direction: ConnDirection) -> ConnId {
        self.conn_seed += 1;
        ConnId::from_raw(0, direction, self.conn_seed)
    }
}

#[cfg(test)]
mod tests {

    use std::net::{IpAddr, SocketAddr};

    use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};

    use crate::controller_plane::connections::{ConnectError, ConnectionEvent, ConnectionStats, DisconnectReason, RESEND_CONNECT_MS, TIMEOUT_CONNECT_MS, TIMEOUT_PING_MS};

    use super::{Connections, ControlMsg, Input, Output};

    fn created_connections(in_conns: usize) -> (Connections, Vec<(SocketAddr, NodeId, ConnId)>) {
        let node_id = 1;
        let mut connections = Connections::new(node_id);
        let mut remotes = vec![];
        for incoming_id in 1..=in_conns {
            let remote_node = 10 + incoming_id as u32;
            let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), remote_node as u16);
            let conn_id = ConnId::from_in(0, incoming_id as u64);
            connections.on_event(0, Input::ControlIn(remote_addr, ControlMsg::ConnectRequest { from: remote_node, to: node_id }));
            assert_eq!(connections.pop_output(), Some(Output::ControlOut(remote_addr, ControlMsg::ConnectResponse(Ok(())))));
            assert_eq!(connections.pop_output(), Some(Output::ConnectionEvent(ConnectionEvent::ConnectionEstablished(conn_id))));
            assert_eq!(connections.pop_output(), None);

            remotes.push((remote_addr, remote_node, conn_id));
        }

        assert_eq!(connections.conns.len(), in_conns);
        (connections, remotes)
    }

    #[test]
    fn handle_connect_success() {
        let node_id = 1;
        let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let remote_node = 2;
        let mut connections = Connections::new(node_id);
        connections.on_event(0, Input::ControlIn(remote_addr, ControlMsg::ConnectRequest { from: remote_node, to: node_id }));
        assert_eq!(connections.pop_output(), Some(Output::ControlOut(remote_addr, ControlMsg::ConnectResponse(Ok(())))));
        assert_eq!(connections.pop_output(), Some(Output::ConnectionEvent(ConnectionEvent::ConnectionEstablished(ConnId::from_in(0, 1)))));
        assert_eq!(connections.pop_output(), None);
        assert_eq!(connections.conns.len(), 1);
    }

    #[test]
    fn handle_connect_wrong_destination() {
        let node_id = 1;
        let other_node_id = 3;
        let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let remote_node = 2;
        let mut connections = Connections::new(node_id);
        connections.on_event(0, Input::ControlIn(remote_addr, ControlMsg::ConnectRequest { from: remote_node, to: other_node_id }));
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remote_addr, ControlMsg::ConnectResponse(Err(ConnectError::WrongDestination))))
        );
        assert_eq!(connections.pop_output(), None);
        assert_eq!(connections.conns.len(), 0);
    }

    #[test]
    fn handle_connect_error_already_connected() {
        let node_id = 1;
        let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let remote_node = 2;
        let mut connections = Connections::new(node_id);
        connections.on_event(0, Input::ControlIn(remote_addr, ControlMsg::ConnectRequest { from: remote_node, to: node_id }));
        assert_eq!(connections.pop_output(), Some(Output::ControlOut(remote_addr, ControlMsg::ConnectResponse(Ok(())))));
        assert_eq!(connections.pop_output(), Some(Output::ConnectionEvent(ConnectionEvent::ConnectionEstablished(ConnId::from_in(0, 1)))));
        assert_eq!(connections.pop_output(), None);

        connections.on_event(0, Input::ControlIn(remote_addr, ControlMsg::ConnectRequest { from: remote_node, to: node_id }));
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remote_addr, ControlMsg::ConnectResponse(Err(ConnectError::AlreadyConnected))))
        );
        assert_eq!(connections.pop_output(), None);
        assert_eq!(connections.conns.len(), 1);
    }

    #[test]
    fn resend_connect() {
        let node_id = 1;
        let remote_node_addr: NodeAddr = "2@/ip4/1.2.3.4/udp/1234".parse().expect("");
        let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let remote_node = 2;
        let mut connections = Connections::new(node_id);
        connections.on_event(0, Input::ConnectTo(remote_node_addr));
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remote_addr, ControlMsg::ConnectRequest { from: node_id, to: remote_node }))
        );
        assert_eq!(connections.pop_output(), None);

        connections.on_tick(RESEND_CONNECT_MS);
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remote_addr, ControlMsg::ConnectRequest { from: node_id, to: remote_node }))
        );
        assert_eq!(connections.pop_output(), None);
    }

    #[test]
    fn send_connect_success() {
        let node_id = 1;
        let remote_node_addr: NodeAddr = "2@/ip4/1.2.3.4/udp/1234".parse().expect("");
        let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let remote_node = 2;
        let mut connections = Connections::new(node_id);
        connections.on_event(0, Input::ConnectTo(remote_node_addr));
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remote_addr, ControlMsg::ConnectRequest { from: node_id, to: remote_node }))
        );
        assert_eq!(connections.pop_output(), None);

        connections.on_event(0, Input::ControlIn(remote_addr, ControlMsg::ConnectResponse(Ok(()))));
        assert_eq!(connections.pop_output(), Some(Output::ConnectionEvent(ConnectionEvent::ConnectionEstablished(ConnId::from_out(0, 1)))));
        assert_eq!(connections.pop_output(), None);
        assert_eq!(connections.outgoings.len(), 0);
        assert_eq!(connections.conns.len(), 1);
    }

    #[test]
    fn send_connect_error() {
        let node_id = 1;
        let remote_node_addr: NodeAddr = "2@/ip4/1.2.3.4/udp/1234".parse().expect("");
        let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let remote_node = 2;
        let mut connections = Connections::new(node_id);
        connections.on_event(0, Input::ConnectTo(remote_node_addr));
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remote_addr, ControlMsg::ConnectRequest { from: node_id, to: remote_node }))
        );
        assert_eq!(connections.pop_output(), None);

        connections.on_event(0, Input::ControlIn(remote_addr, ControlMsg::ConnectResponse(Err(ConnectError::WrongDestination))));
        assert_eq!(connections.pop_output(), None);
        assert_eq!(connections.outgoings.len(), 0);
    }

    #[test]
    fn send_connect_timeout() {
        let node_id = 1;
        let remote_node_addr: NodeAddr = "2@/ip4/1.2.3.4/udp/1234".parse().expect("");
        let remote_addr = SocketAddr::new(IpAddr::from([1, 2, 3, 4]), 1234);
        let remote_node = 2;
        let mut connections = Connections::new(node_id);
        connections.on_event(0, Input::ConnectTo(remote_node_addr));
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remote_addr, ControlMsg::ConnectRequest { from: node_id, to: remote_node }))
        );
        assert_eq!(connections.pop_output(), None);

        connections.on_tick(TIMEOUT_CONNECT_MS);
        assert_eq!(connections.pop_output(), None);
        assert_eq!(connections.outgoings.len(), 0);
    }

    #[test]
    fn ping_pong_success() {
        let (mut connections, remotes) = created_connections(1);

        connections.on_tick(1000);
        assert_eq!(connections.pop_output(), Some(Output::ControlOut(remotes[0].0, ControlMsg::Ping(0, 1000))));
        assert_eq!(connections.pop_output(), None);

        connections.on_event(1100, Input::ControlIn(remotes[0].0, ControlMsg::Pong(0, 1000)));
        assert_eq!(
            connections.pop_output(),
            Some(Output::ConnectionEvent(ConnectionEvent::ConnectionStats(remotes[0].2, ConnectionStats { rtt: 100 })))
        );
        assert_eq!(connections.pop_output(), None);
    }

    #[test]
    fn ping_pong_timeout() {
        let (mut connections, remotes) = created_connections(1);

        connections.on_tick(1000);
        assert_eq!(connections.pop_output(), Some(Output::ControlOut(remotes[0].0, ControlMsg::Ping(0, 1000))));
        assert_eq!(connections.pop_output(), None);

        connections.on_tick(1000 + TIMEOUT_PING_MS);
        assert_eq!(connections.pop_output(), Some(Output::ConnectionEvent(ConnectionEvent::ConnectionDisconnected(remotes[0].2))));
        assert_eq!(connections.pop_output(), None);
    }

    #[test]
    fn disconnect_node_success() {
        let (mut connections, remotes) = created_connections(1);

        connections.disconnect_from(remotes[0].1);
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remotes[0].0, ControlMsg::DisconnectRequest(DisconnectReason::RemoteDisconnect)))
        );
        assert_eq!(connections.pop_output(), None);

        connections.on_event(10, Input::ControlIn(remotes[0].0, ControlMsg::DisconnectResponse(Ok(()))));
        assert_eq!(connections.pop_output(), Some(Output::ConnectionEvent(ConnectionEvent::ConnectionDisconnected(remotes[0].2))));
        assert_eq!(connections.pop_output(), None);
    }

    #[test]
    fn disconnect_node_resend() {
        let (mut connections, remotes) = created_connections(1);

        connections.disconnect_from(remotes[0].1);
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remotes[0].0, ControlMsg::DisconnectRequest(DisconnectReason::RemoteDisconnect)))
        );
        assert_eq!(connections.pop_output(), None);

        connections.on_tick(10);
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remotes[0].0, ControlMsg::DisconnectRequest(DisconnectReason::RemoteDisconnect)))
        );
        assert_eq!(connections.pop_output(), None);
    }

    #[test]
    fn disconnect_node_timeout() {
        let (mut connections, remotes) = created_connections(1);

        connections.disconnect_from(remotes[0].1);
        assert_eq!(
            connections.pop_output(),
            Some(Output::ControlOut(remotes[0].0, ControlMsg::DisconnectRequest(DisconnectReason::RemoteDisconnect)))
        );
        assert_eq!(connections.pop_output(), None);

        connections.on_tick(TIMEOUT_PING_MS);
        assert_eq!(connections.pop_output(), Some(Output::ConnectionEvent(ConnectionEvent::ConnectionDisconnected(remotes[0].2))));
        assert_eq!(connections.pop_output(), None);
    }
}

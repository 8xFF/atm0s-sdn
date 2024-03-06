//! Transport manager with take care create outgoing connections, processing handshake.
//! It also take care handling incoming packets and processing handshake.
//! When connection is successfully established, it will be passed to all background workers.
//! After that, it only save the connection a few seconds incase some packets are still forward to it.

use std::{collections::{HashMap, VecDeque}, net::{IpAddr, SocketAddr}};

use atm0s_sdn_identity::{ConnDirection, ConnId, NodeAddr, NodeId, Protocol};

use super::{conn_addr::ConnAddr, msg::TransportMsg, ConnectionInfo, OutgoingConnectionError, TransportBuffer, TransportEvent, TransportHandshakeRequest, TransportHandshakeResult, TransportMetaMsg, TransportProtocol};

const RESEND_HANDSHAKE_MS: u64 = 500;
const TIMEOUT_HANDSHAKE_MS: u64 = 10000;
const CONNECTION_REMAIN_MS: u64 = 5000;

pub enum TransportManagerInput<'a> {
    OutgoingRequest(NodeAddr),
    RecvFrom(TransportProtocol, SocketAddr, TransportBuffer<'a>),
}

pub enum TransportManagerOut<'a> {
    Connect(TransportProtocol, SocketAddr),
    SendTo(TransportProtocol, SocketAddr, TransportBuffer<'a>),
    PassConnection(ConnId, TransportMsg<'a>),
    Event(TransportEvent),
}

struct ConnectionSlot {
    id: ConnId,
    addr: ConnAddr,
    created_at: u64,
}

struct OutgoingSlot {
    conn_id: ConnId,
    dest_node: NodeId,
    dest_addr: NodeAddr,
    created_at: u64,
    last_send: u64,
    sent_count: u8,
}

pub struct TransportManager<'a> {
    node_addr: NodeAddr,
    uuid_seed: u64,
    conns: HashMap<ConnAddr, ConnectionSlot>,
    outgoings: HashMap<ConnAddr, OutgoingSlot>,
    output: VecDeque<TransportManagerOut<'a>>,
}

impl<'a> TransportManager<'a> {
    fn conn_id_gen(&mut self, protocol: TransportProtocol, direction: ConnDirection) -> ConnId {
        let uuid = self.uuid_seed;
        self.uuid_seed += 1;
        let protocol_id = match protocol {
            TransportProtocol::Udp => 0,
            TransportProtocol::Tcp => 1,
        };
        ConnId::from_raw(protocol_id, direction, uuid)
    }

    fn send_handshake(&self, now_ms: u64, protocol: TransportProtocol, dest_addr: SocketAddr, slot: &mut OutgoingSlot) {
        slot.sent_count += 1;
        slot.last_send = now_ms;
        let handshake = TransportMetaMsg::ConnectRequest(TransportHandshakeRequest {
            from_addr: self.node_addr,
            to_node: slot.dest_node,
            ts: now_ms,
            count: slot.sent_count,
        });
        let buf = bincode::serialize(&handshake).expect("Should convert to bytes");
        
        self.output.push_back(TransportManagerOut::SendTo(protocol, dest_addr, TransportBuffer::Vec(buf)));
    }

    fn process_handshake_req(&mut self, now_ms: u64, from_addr: SocketAddr, protocol: TransportProtocol, req: TransportHandshakeRequest) -> Option<TransportManagerOut> {
        let res = if req.to_node == self.node_addr.node_id() {
            let conn_id = self.conn_id_gen(protocol, ConnDirection::Incoming);
            self.conns.insert(ConnAddr(protocol, from_addr), ConnectionSlot {
                id: conn_id,
                addr: ConnAddr(protocol, from_addr),
                created_at: now_ms,
            });
            self.output.push_back(TransportManagerOut::Event(TransportEvent::Incoming(ConnectionInfo {
                outgoing: false,
                remote_node: req.from_addr.node_id(),
                conn_id,
                remote_addr: req.from_addr,
                socket_addr: from_addr,
                protocol,
            })));
            TransportMetaMsg::ConnectResponse(TransportHandshakeResult::Success)
        } else {
            TransportMetaMsg::ConnectResponse(TransportHandshakeResult::Rejected)
        };
        let buf = bincode::serialize(&res).expect("Should convert to bytes");
        Some(TransportManagerOut::SendTo(protocol, from_addr, TransportBuffer::Vec(buf)))
    }

    fn process_handshake_res(&mut self, now_ms: u64, protocol: TransportProtocol, from_addr: SocketAddr, res: TransportHandshakeResult) -> Option<TransportManagerOut> {
        let conn_addr = ConnAddr(protocol, from_addr);
        let slot = self.outgoings.remove(&conn_addr)?;

        match res {
            TransportHandshakeResult::Success => {
                self.conns.insert(conn_addr, ConnectionSlot {
                    id: slot.conn_id,
                    addr: ConnAddr(protocol, from_addr),
                    created_at: now_ms,
                });
                Some(TransportManagerOut::Event(TransportEvent::Outgoing(ConnectionInfo {
                    outgoing: true,
                    remote_node: slot.dest_node,
                    conn_id: slot.conn_id,
                    remote_addr: slot.dest_addr,
                    socket_addr: from_addr,
                    protocol,
                })))
            },
            TransportHandshakeResult::Rejected => {
                Some(TransportManagerOut::Event(TransportEvent::OutgoingError {
                    node_id: slot.dest_node,
                    conn_id: slot.conn_id,
                    err: OutgoingConnectionError::AuthenticationError,
                }))
            },
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) -> Option<TransportManagerOut> {
        for (conn_addr, slot) in &mut self.outgoings {
            if now_ms - slot.last_send >= TIMEOUT_HANDSHAKE_MS {
                self.outgoings.remove(conn_addr);
                self.output.push_back(TransportManagerOut::Event(TransportEvent::OutgoingError { node_id: slot.dest_node, conn_id: slot.conn_id, err: OutgoingConnectionError::ConnectionTimeout }));
                continue;
            }
            if now_ms - slot.last_send >= RESEND_HANDSHAKE_MS {
                self.send_handshake(now_ms, conn_addr.0, conn_addr.1, slot);
            }
        }
        for (conn_addr, slot) in &mut self.conns {
            if now_ms - slot.created_at >= CONNECTION_REMAIN_MS {
                self.conns.remove(conn_addr);
            }
        }
        self.output.pop_front()
    }

    pub fn on_event(&mut self, now_ms: u64, event: TransportManagerInput) -> Option<TransportManagerOut> {
        let mut create_outgoing = |dest_node: NodeId, dest_addr: NodeAddr, protocol: TransportProtocol, socket_addr: SocketAddr| {
            let conn_id = self.conn_id_gen(protocol, ConnDirection::Outgoing);
            let mut slot = OutgoingSlot {
                created_at: now_ms,
                last_send: now_ms,
                conn_id,
                dest_node,
                sent_count: 0,
                dest_addr,
            };
            self.send_handshake(now_ms, protocol, socket_addr, &mut slot);
            self.outgoings.insert(ConnAddr(protocol, socket_addr), slot);
        };

        match event {
            TransportManagerInput::OutgoingRequest(dest) => {
                let dest_node = dest.node_id();
                let mut ip_addr = None;
                for proto in dest.multiaddr().iter() {
                    match proto {
                        Protocol::Ip4(ip) => {
                            ip_addr = Some(IpAddr::V4(ip));
                        }
                        Protocol::Ip6(ip) => {
                            ip_addr = Some(IpAddr::V6(ip));
                        }
                        Protocol::Udp(portnum) => match &ip_addr {
                            Some(ip) => {
                                create_outgoing(dest_node, dest.clone(), SocketAddr::new(*ip, portnum), TransportProtocol::Udp);
                            }
                            None => {}
                        },
                        Protocol::Tcp(portnum) => match &ip_addr {
                            Some(ip) => {
                                create_outgoing(dest_node, dest.clone(), SocketAddr::new(*ip, portnum), TransportProtocol::Tcp);
                            }
                            None => {}
                        }
                        _ => {}
                    }
                }
                self.output.pop_front()
            },
            TransportManagerInput::RecvFrom(protocol, from, data) => {
                if let Some(slot) = self.conns.get_mut(&ConnAddr(protocol, from)) {
                    Some(TransportManagerOut::PassConnection(slot.id, data))
                } else if let Some(slot) = self.outgoings.get_mut(&(from, protocol)) {
                    let meta = bincode::deserialize::<TransportMetaMsg>(&data).expect("Should convert to handshake");
                    match meta {
                        TransportMetaMsg::ConnectRequest(req) => {
                            self.process_handshake_req(now_ms, from, protocol, req)
                        },
                        TransportMetaMsg::ConnectResponse(res) => {
                            self.process_handshake_res(now_ms, from, protocol, res)
                        },
                        _ => {
                            None
                        }
                    }
                } else {
                    None
                }
            },
        }
    }
    pub fn pop_output(&mut self) -> Option<TransportManagerOut> {
        self.output.pop_front()
    }
}

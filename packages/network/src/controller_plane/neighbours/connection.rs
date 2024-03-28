use std::{collections::VecDeque, net::SocketAddr, sync::Arc};

use atm0s_sdn_identity::{ConnId, NodeId};

use crate::base::{ConnectionCtx, ConnectionStats, Decryptor, Encryptor, HandshakeBuilder, HandshakeRequester, NeighboursConnectError, NeighboursControlCmds, NeighboursDisconnectReason};

const INIT_RTT_MS: u32 = 1000;
const RETRY_CMD_MS: u64 = 1000;
const TIMEOUT_MS: u64 = 10000;

enum State {
    Connecting {
        at_ms: u64,
        requester: Option<Box<dyn HandshakeRequester>>,
    },
    ConnectError(NeighboursConnectError),
    ConnectTimeout,
    Connected {
        last_pong_ms: u64,
        ping_seq: u64,
        stats: ConnectionStats,
        encryptor: Box<dyn Encryptor>,
        decryptor: Box<dyn Decryptor>,
    },
    Disconnecting {
        at_ms: u64,
    },
    Disconnected,
}

pub enum ConnectionEvent {
    Connected(Box<dyn Encryptor>, Box<dyn Decryptor>),
    ConnectError(NeighboursConnectError),
    ConnectTimeout,
    Stats(ConnectionStats),
    Disconnected,
}

pub enum Output {
    Event(ConnectionEvent),
    Net(SocketAddr, NeighboursControlCmds),
}

pub struct NeighbourConnection {
    conn: ConnId,
    local: NodeId,
    node: NodeId,
    remote: SocketAddr,
    state: State,
    output: VecDeque<Output>,
    handshake_builder: Arc<dyn HandshakeBuilder>,
}

impl NeighbourConnection {
    pub fn new_outgoing(handshake_builder: Arc<dyn HandshakeBuilder>, local: NodeId, node: NodeId, session: u64, remote: SocketAddr, now_ms: u64) -> Self {
        let requester = handshake_builder.requester();
        let handshake = requester.create_public_request().expect("Should have handshake");
        let state = State::Connecting {
            at_ms: now_ms,
            requester: Some(requester),
        };
        Self {
            conn: ConnId::from_out(0, session),
            local,
            node,
            remote,
            state,
            output: VecDeque::from([Output::Net(remote, NeighboursControlCmds::ConnectRequest { to: node, session, handshake })]),
            handshake_builder,
        }
    }

    pub fn new_incoming(handshake_builder: Arc<dyn HandshakeBuilder>, local: NodeId, node: NodeId, session: u64, remote: SocketAddr, now_ms: u64) -> Self {
        let state: State = State::Connecting { at_ms: now_ms, requester: None };
        Self {
            conn: ConnId::from_in(0, session),
            local,
            node,
            remote,
            state,
            output: VecDeque::new(),
            handshake_builder,
        }
    }

    pub fn dest_node(&self) -> NodeId {
        self.node
    }

    pub fn ctx(&self) -> ConnectionCtx {
        ConnectionCtx {
            conn: self.conn,
            node: self.node,
            remote: self.remote,
        }
    }

    pub fn disconnect(&mut self, now_ms: u64) {
        match &mut self.state {
            State::Connecting { .. } | State::Connected { .. } => {
                log::info!("[NeighbourConnection] Sending disconnect request with remote {}", self.remote);
                self.state = State::Disconnecting { at_ms: now_ms };
                self.output.push_back(self.generate_control(NeighboursControlCmds::DisconnectRequest {
                    session: self.conn.session(),
                    reason: NeighboursDisconnectReason::Other,
                }));
            }
            _ => {
                log::warn!("[NeighbourConnection] Invalid state for performing disconnect request with remote {}", self.remote);
            }
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        match &mut self.state {
            State::Connecting { at_ms, requester } => {
                if let Some(requester) = requester {
                    if now_ms - *at_ms >= TIMEOUT_MS {
                        self.state = State::ConnectTimeout;
                        self.output.push_back(Output::Event(ConnectionEvent::ConnectTimeout));
                        log::warn!("[NeighbourConnection] Connection timeout to {} after {} ms", self.remote, TIMEOUT_MS);
                    } else if now_ms - *at_ms >= RETRY_CMD_MS {
                        if let Ok(request_buf) = requester.create_public_request() {
                            self.output.push_back(self.generate_control(NeighboursControlCmds::ConnectRequest {
                                to: self.node,
                                session: self.conn.session(),
                                handshake: request_buf,
                            }));
                            log::info!("[NeighbourConnection] Resend connect request to {}, dest_node {}", self.remote, self.node);
                        } else {
                            log::warn!(
                                "[NeighbourConnection] Cannot create handshake for resending connect request to {}, dest_node {}",
                                self.remote,
                                self.node
                            );
                        }
                    }
                }
            }
            State::Connected { ping_seq, last_pong_ms, .. } => {
                if now_ms - *last_pong_ms >= TIMEOUT_MS {
                    log::warn!("[NeighbourConnection] Connection timeout to {} after a while not received pong, last {last_pong_ms}", self.remote);
                    self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                } else {
                    log::debug!("[NeighbourConnection] Send ping to {}", self.remote);
                    *ping_seq += 1;
                    let cmd = NeighboursControlCmds::Ping {
                        session: self.conn.session(),
                        seq: *ping_seq,
                        sent_ms: now_ms,
                    };
                    self.output.push_back(self.generate_control(cmd));
                }
            }
            State::Disconnecting { at_ms } => {
                if now_ms - *at_ms >= TIMEOUT_MS {
                    self.state = State::Disconnected;
                    self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                    log::warn!("[NeighbourConnection] Disconnect request timeout to {} after {} ms", self.remote, TIMEOUT_MS);
                } else {
                    *at_ms = now_ms;
                    self.output.push_back(self.generate_control(NeighboursControlCmds::DisconnectRequest {
                        session: self.conn.session(),
                        reason: NeighboursDisconnectReason::Other,
                    }));
                    log::info!("Resend disconnect request to {}", self.remote);
                }
            }
            _ => {}
        }
    }

    pub fn on_input(&mut self, now_ms: u64, from: NodeId, cmd: NeighboursControlCmds) {
        match cmd {
            NeighboursControlCmds::ConnectRequest { to, session, handshake } => {
                let result = if self.local == to && self.node == from {
                    if let State::Connecting { requester, .. } = &mut self.state {
                        if requester.is_none() || self.conn.session() >= session {
                            //check if we can replace the existing connection to accept the new one
                            if self.conn.session() >= session {
                                log::warn!(
                                    "[NeighbourConnection] Conflic state from {}, local session {}, remote session {} => switch to incoming",
                                    self.remote,
                                    self.conn.session(),
                                    session
                                );
                                self.switch_to_incoming(session);
                            }

                            let mut responder = self.handshake_builder.responder();
                            match responder.process_public_request(&handshake) {
                                Ok((encryptor, decryptor, response)) => {
                                    self.output.push_back(Output::Event(ConnectionEvent::Connected(encryptor.clone_box(), decryptor.clone_box())));
                                    self.state = State::Connected {
                                        last_pong_ms: now_ms,
                                        ping_seq: 0,
                                        stats: ConnectionStats { rtt_ms: INIT_RTT_MS },
                                        encryptor,
                                        decryptor,
                                    };
                                    log::info!("Connected to {} as incoming conn", self.remote);
                                    Ok(response)
                                }
                                Err(_) => Err(NeighboursConnectError::InvalidData),
                            }
                        } else {
                            log::warn!(
                                "[NeighbourConnection] Conflic state from {}, local session {}, remote session {} => don't switch to incoming",
                                self.remote,
                                self.conn.session(),
                                session
                            );
                            return;
                        }
                    } else {
                        log::warn!("[NeighbourConnection] Invalid state, should be Connecting for connect request from {}", self.remote);
                        Err(NeighboursConnectError::InvalidState)
                    }
                } else {
                    log::warn!(
                        "[NeighbourConnection] Invalid from or to in connect request from {}, {} vs {}, {} vs {}",
                        self.remote,
                        self.local,
                        to,
                        self.node,
                        from
                    );
                    Err(NeighboursConnectError::InvalidData)
                };
                self.output.push_back(self.generate_control(NeighboursControlCmds::ConnectResponse { session, result }));
            }
            NeighboursControlCmds::ConnectResponse { session, result } => {
                if session == self.conn.session() {
                    if let State::Connecting { requester, .. } = &mut self.state {
                        match (requester, result) {
                            (Some(requester), Ok(handshake_res)) => match requester.process_public_response(&handshake_res) {
                                Ok((encryptor, decryptor)) => {
                                    self.output.push_back(Output::Event(ConnectionEvent::Connected(encryptor.clone_box(), decryptor.clone_box())));
                                    self.state = State::Connected {
                                        last_pong_ms: now_ms,
                                        ping_seq: 0,
                                        stats: ConnectionStats { rtt_ms: INIT_RTT_MS },
                                        encryptor,
                                        decryptor,
                                    };
                                    log::info!("Connected to {} as outgoing conn", self.remote);
                                }
                                Err(e) => {
                                    log::warn!("Connect response from  {} but handshake error {:?}", self.remote, e);
                                    self.state = State::ConnectError(NeighboursConnectError::InvalidData);
                                    self.output.push_back(Output::Event(ConnectionEvent::ConnectError(NeighboursConnectError::InvalidData)));
                                }
                            },
                            (None, Ok(_)) => {
                                log::warn!("Connect response from  {} but wrong state, handshake requester not found", self.remote);
                                self.state = State::ConnectError(NeighboursConnectError::InvalidData);
                                self.output.push_back(Output::Event(ConnectionEvent::ConnectError(NeighboursConnectError::InvalidData)));
                            }
                            (_, Err(err)) => {
                                log::warn!("Connect response error from {}: {:?}", self.remote, err);
                                self.state = State::ConnectError(err);
                                self.output.push_back(Output::Event(ConnectionEvent::ConnectError(err)));
                            }
                        }
                    } else {
                        log::warn!("[NeighbourConnection] Invalid state, should Connecting for connect response from {}", self.remote);
                    }
                } else {
                    log::warn!("[NeighbourConnection] Invalid session in connect response from {}", self.remote);
                }
            }
            NeighboursControlCmds::Ping { session, seq, sent_ms } => {
                if session == self.conn.session() {
                    if let State::Connected { .. } = &self.state {
                        self.output.push_back(self.generate_control(NeighboursControlCmds::Pong { session, seq, sent_ms }));
                    } else {
                        log::warn!("[NeighbourConnection] Invalid state, should be Connected for ping from {}", self.remote);
                    }
                } else {
                    log::warn!("[NeighbourConnection] Invalid session in ping from {}", self.remote);
                }
            }
            NeighboursControlCmds::Pong { session, sent_ms, .. } => {
                if session == self.conn.session() {
                    if let State::Connected { last_pong_ms, stats, .. } = &mut self.state {
                        *last_pong_ms = now_ms;
                        if sent_ms <= now_ms {
                            stats.rtt_ms = (now_ms - sent_ms) as u32;
                            self.output.push_back(Output::Event(ConnectionEvent::Stats(stats.clone())));
                            log::trace!("Received pong from {} after {}", self.remote, stats.rtt_ms);
                        } else {
                            log::warn!("[NeighbourConnection] Invalid sent_ms in pong from {}", self.remote);
                        }
                    } else {
                        log::warn!("[NeighbourConnection] Invalid state, should be Connected for ping from {}", self.remote);
                    }
                } else {
                    log::warn!("[NeighbourConnection] Invalid session in ping from {}", self.remote);
                }
            }
            NeighboursControlCmds::DisconnectRequest { session, .. } => {
                if session == self.conn.session() {
                    self.state = State::Disconnected;
                    self.output.push_back(self.generate_control(NeighboursControlCmds::DisconnectResponse { session }));
                    self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                    log::info!("[NeighbourConnection] Disconnect request from {}", self.remote);
                } else {
                    log::warn!("[NeighbourConnection] Invalid session in disconnect request from {}", self.remote);
                }
            }
            NeighboursControlCmds::DisconnectResponse { session } => {
                if session == self.conn.session() {
                    if let State::Disconnecting { .. } = self.state {
                        self.state = State::Disconnected;
                        self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                        log::info!("[NeighbourConnection] Disconnected response from {}", self.remote);
                    } else {
                        log::warn!("[NeighbourConnection] Invalid state, should be Disconnecting for disconnect response from {}", self.remote);
                    }
                } else {
                    log::warn!("[NeighbourConnection] Invalid session in disconnect response from {}", self.remote);
                }
            }
        }
    }

    pub fn pop_output(&mut self) -> Option<Output> {
        self.output.pop_front()
    }

    fn generate_control(&self, control: NeighboursControlCmds) -> Output {
        Output::Net(self.remote, control)
    }

    fn switch_to_incoming(&mut self, session: u64) {
        let old = self.conn;
        self.conn = ConnId::from_in(0, session);
        log::warn!("Switching to incoming connection from {}, rewriting conn from {old} to {}", self.remote, self.conn);
    }
}

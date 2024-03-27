use std::{collections::VecDeque, net::SocketAddr};

use atm0s_sdn_identity::{ConnId, NodeId};

use crate::base::{ConnectionCtx, ConnectionStats, NeighboursConnectError, NeighboursControlCmds, NeighboursDisconnectReason};

const INIT_RTT_MS: u32 = 1000;
const RETRY_CMD_MS: u64 = 1000;
const TIMEOUT_MS: u64 = 10000;

enum State {
    Connecting { at_ms: u64, client: bool },
    ConnectError(NeighboursConnectError),
    ConnectTimeout,
    Connected { last_pong_ms: u64, ping_seq: u64, stats: ConnectionStats },
    Disconnecting { at_ms: u64 },
    Disconnected,
}

pub enum ConnectionEvent {
    Connected,
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
}

impl NeighbourConnection {
    pub fn new_outgoing(local: NodeId, node: NodeId, session: u64, remote: SocketAddr, now_ms: u64) -> Self {
        let state = State::Connecting { at_ms: now_ms, client: true };
        Self {
            conn: ConnId::from_out(0, session),
            local,
            node,
            remote,
            state,
            output: VecDeque::from([Output::Net(remote, NeighboursControlCmds::ConnectRequest { to: node, session })]),
        }
    }

    pub fn new_incoming(local: NodeId, node: NodeId, session: u64, remote: SocketAddr, now_ms: u64) -> Self {
        let state: State = State::Connecting { at_ms: now_ms, client: false };
        Self {
            conn: ConnId::from_in(0, session),
            local,
            node,
            remote,
            state,
            output: VecDeque::new(),
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
            State::Connecting { at_ms, client } => {
                if *client && now_ms - *at_ms >= TIMEOUT_MS {
                    self.state = State::ConnectTimeout;
                    self.output.push_back(Output::Event(ConnectionEvent::ConnectTimeout));
                    log::warn!("[NeighbourConnection] Connection timeout to {} after {} ms", self.remote, TIMEOUT_MS);
                } else if *client && now_ms - *at_ms >= RETRY_CMD_MS {
                    self.output.push_back(self.generate_control(NeighboursControlCmds::ConnectRequest {
                        to: self.node,
                        session: self.conn.session(),
                    }));
                    log::info!("[NeighbourConnection] Resend connect request to {}, dest_node {}", self.remote, self.node);
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
        // TODO checking signature here
        match cmd {
            NeighboursControlCmds::ConnectRequest { to, session } => {
                let result = if self.local == to && self.node == from {
                    if let State::Connecting { client, .. } = self.state {
                        if !client || self.conn.session() >= session {
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

                            self.state = State::Connected {
                                last_pong_ms: now_ms,
                                ping_seq: 0,
                                stats: ConnectionStats { rtt_ms: INIT_RTT_MS },
                            };
                            self.output.push_back(Output::Event(ConnectionEvent::Connected));
                            log::info!("Connected to {} as incoming conn", self.remote);
                            Ok(())
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
                    if let State::Connecting { .. } = self.state {
                        match result {
                            Ok(()) => {
                                self.state = State::Connected {
                                    last_pong_ms: now_ms,
                                    ping_seq: 0,
                                    stats: ConnectionStats { rtt_ms: INIT_RTT_MS },
                                };
                                self.output.push_back(Output::Event(ConnectionEvent::Connected));
                                log::info!("Connected to {} as outgoing conn", self.remote);
                            }
                            Err(err) => {
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

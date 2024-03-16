use std::{collections::VecDeque, net::SocketAddr};

use atm0s_sdn_identity::{ConnId, NodeId};

use crate::base::{ConnectionCtx, ConnectionStats, NeigboursControlCmds, NeighboursConnectError, NeighboursControl, NeighboursDisconnectReason};

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
    Net(SocketAddr, NeighboursControl),
}

pub struct NeighbourConnection {
    conn: ConnId,
    local: NodeId,
    node: NodeId,
    session: u64,
    remote: SocketAddr,
    state: State,
    output: VecDeque<Output>,
}

impl NeighbourConnection {
    pub fn new_outgoing(conn: ConnId, local: NodeId, node: NodeId, session: u64, remote: SocketAddr, now_ms: u64) -> Self {
        let state = State::Connecting { at_ms: now_ms, client: true };
        Self {
            conn,
            local,
            node,
            session,
            remote,
            state,
            output: VecDeque::from([Output::Net(
                remote,
                NeighboursControl {
                    ts: now_ms,
                    cmd: NeigboursControlCmds::ConnectRequest { from: local, to: node, session },
                    signature: vec![],
                },
            )]),
        }
    }

    pub fn new_incoming(conn: ConnId, local: NodeId, node: NodeId, session: u64, remote: SocketAddr, now_ms: u64) -> Self {
        let state: State = State::Connecting { at_ms: now_ms, client: false };
        Self {
            conn,
            local,
            node,
            session,
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
            session: self.session,
            conn: self.conn,
            node: self.node,
            remote: self.remote,
        }
    }

    pub fn disconnect(&mut self, now_ms: u64) {
        match &mut self.state {
            State::Connecting { .. } | State::Connected { .. } => {
                log::info!("Sending disconnect request with remote {}", self.remote);
                self.state = State::Disconnecting { at_ms: now_ms };
                self.output.push_back(self.generate_control(
                    now_ms,
                    NeigboursControlCmds::DisconnectRequest {
                        session: self.session,
                        reason: NeighboursDisconnectReason::Other,
                    },
                ));
            }
            _ => {
                log::warn!("Invalid state for performing disconnect request with remote {}", self.remote);
            }
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        match &mut self.state {
            State::Connecting { at_ms, client } => {
                if *client && now_ms - *at_ms >= TIMEOUT_MS {
                    self.state = State::ConnectTimeout;
                    self.output.push_back(Output::Event(ConnectionEvent::ConnectTimeout));
                    log::warn!("Connection timeout to {} after {} ms", self.remote, TIMEOUT_MS);
                } else if *client && now_ms - *at_ms >= RETRY_CMD_MS {
                    self.output.push_back(self.generate_control(
                        now_ms,
                        NeigboursControlCmds::ConnectRequest {
                            from: self.local,
                            to: self.node,
                            session: self.session,
                        },
                    ));
                    log::info!("Resend connect request to {}, dest_node {}", self.remote, self.node);
                }
            }
            State::Connected { ping_seq, last_pong_ms, .. } => {
                if now_ms - *last_pong_ms >= TIMEOUT_MS {
                    log::warn!("Connection timeout to {} after a while not received pong, last {last_pong_ms}", self.remote);
                    self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                } else {
                    *ping_seq += 1;
                    let cmd = NeigboursControlCmds::Ping {
                        session: self.session,
                        seq: *ping_seq,
                        sent_ms: now_ms,
                    };
                    self.output.push_back(self.generate_control(now_ms, cmd));
                }
            }
            State::Disconnecting { at_ms } => {
                if now_ms - *at_ms >= TIMEOUT_MS {
                    self.state = State::Disconnected;
                    self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                    log::warn!("Disconnect request timeout to {} after {} ms", self.remote, TIMEOUT_MS);
                } else {
                    *at_ms = now_ms;
                    self.output.push_back(self.generate_control(
                        now_ms,
                        NeigboursControlCmds::DisconnectRequest {
                            session: self.session,
                            reason: NeighboursDisconnectReason::Other,
                        },
                    ));
                    log::info!("Resend disconnect request to {}", self.remote);
                }
            }
            _ => {}
        }
    }

    pub fn on_input(&mut self, now_ms: u64, input: NeighboursControl) {
        // TODO checking signature here
        match input.cmd {
            NeigboursControlCmds::ConnectRequest { from, to, session } => {
                let result = if self.local == to && self.node == from {
                    if let State::Connecting { client, .. } = self.state {
                        if !client {
                            self.state = State::Connected {
                                last_pong_ms: now_ms,
                                ping_seq: 0,
                                stats: ConnectionStats { rtt_ms: INIT_RTT_MS },
                            };
                            self.output.push_back(Output::Event(ConnectionEvent::Connected));
                            log::info!("Connected to {} as incomming conn", self.remote);
                            Ok(())
                        } else {
                            log::warn!("Should be incomming conn for processing connect request from {}", self.remote);
                            Err(NeighboursConnectError::InvalidState)
                        }
                    } else {
                        log::warn!("Invalid state, should be Connecting for connect request from {}", self.remote);
                        Err(NeighboursConnectError::InvalidState)
                    }
                } else {
                    log::warn!("Invalid from or to in connect request from {}, {} vs {}, {} vs {}", self.remote, self.local, to, self.node, from);
                    Err(NeighboursConnectError::InvalidData)
                };
                self.output.push_back(self.generate_control(now_ms, NeigboursControlCmds::ConnectResponse { session, result }));
            }
            NeigboursControlCmds::ConnectResponse { session, result } => {
                if session == self.session {
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
                        log::warn!("Invalid state, should Connecting for connect response from {}", self.remote);
                    }
                } else {
                    log::warn!("Invalid session in connect response from {}", self.remote);
                }
            }
            NeigboursControlCmds::Ping { session, seq, sent_ms } => {
                if session == self.session {
                    if let State::Connected { .. } = &self.state {
                        self.output.push_back(self.generate_control(now_ms, NeigboursControlCmds::Pong { session, seq, sent_ms }));
                    } else {
                        log::warn!("Invalid state, should be Connected for ping from {}", self.remote);
                    }
                } else {
                    log::warn!("Invalid session in ping from {}", self.remote);
                }
            }
            NeigboursControlCmds::Pong { session, sent_ms, .. } => {
                if session == self.session {
                    if let State::Connected { last_pong_ms, stats, .. } = &mut self.state {
                        *last_pong_ms = now_ms;
                        if sent_ms < now_ms {
                            stats.rtt_ms = (now_ms - sent_ms) as u32;
                            self.output.push_back(Output::Event(ConnectionEvent::Stats(stats.clone())));
                        } else {
                            log::warn!("Invalid sent_ms in pong from {}", self.remote);
                        }
                    } else {
                        log::warn!("Invalid state, should be Connected for ping from {}", self.remote);
                    }
                } else {
                    log::warn!("Invalid session in ping from {}", self.remote);
                }
            }
            NeigboursControlCmds::DisconnectRequest { session, .. } => {
                if session == self.session {
                    self.state = State::Disconnected;
                    self.output.push_back(self.generate_control(now_ms, NeigboursControlCmds::DisconnectResponse { session }));
                    self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                    log::info!("Disconnect request from {}", self.remote);
                } else {
                    log::warn!("Invalid session in disconnect request from {}", self.remote);
                }
            }
            NeigboursControlCmds::DisconnectResponse { session } => {
                if session == self.session {
                    if let State::Disconnecting { .. } = self.state {
                        self.state = State::Disconnected;
                        self.output.push_back(Output::Event(ConnectionEvent::Disconnected));
                        log::info!("Disconnected response from {}", self.remote);
                    } else {
                        log::warn!("Invalid state, should be Disconnecting for disconnect response from {}", self.remote);
                    }
                } else {
                    log::warn!("Invalid session in disconnect response from {}", self.remote);
                }
            }
        }
    }

    pub fn pop_output(&mut self) -> Option<Output> {
        self.output.pop_front()
    }

    fn generate_control(&self, now_ms: u64, control: NeigboursControlCmds) -> Output {
        let signature = vec![];
        Output::Net(self.remote, NeighboursControl { ts: now_ms, cmd: control, signature })
    }
}

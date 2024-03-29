use std::{collections::VecDeque, fmt::Debug, net::SocketAddr, sync::Arc};

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
        /// handshake_req, hanshake_res, remote_session
        handshake: Option<(Vec<u8>, Vec<u8>, u64)>,
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

impl Debug for ConnectionEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionEvent::Connected(_, _) => write!(f, "Connected"),
            ConnectionEvent::ConnectError(err) => write!(f, "ConnectError({:?})", err),
            ConnectionEvent::ConnectTimeout => write!(f, "ConnectTimeout"),
            ConnectionEvent::Stats(_) => write!(f, "Stats"),
            ConnectionEvent::Disconnected => write!(f, "Disconnected"),
        }
    }
}

impl PartialEq for ConnectionEvent {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ConnectionEvent::Connected(_, _), ConnectionEvent::Connected(_, _)) => true,
            (ConnectionEvent::ConnectError(err1), ConnectionEvent::ConnectError(err2)) => err1 == err2,
            (ConnectionEvent::ConnectTimeout, ConnectionEvent::ConnectTimeout) => true,
            (ConnectionEvent::Stats(_), ConnectionEvent::Stats(_)) => true,
            (ConnectionEvent::Disconnected, ConnectionEvent::Disconnected) => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq)]
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
                    match &mut self.state {
                        State::Connecting { requester, .. } => {
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
                                        self.output.push_back(Output::Event(ConnectionEvent::Connected(encryptor, decryptor)));
                                        self.state = State::Connected {
                                            last_pong_ms: now_ms,
                                            ping_seq: 0,
                                            stats: ConnectionStats { rtt_ms: INIT_RTT_MS },
                                            handshake: Some((handshake, response.clone(), session)),
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
                        }
                        State::Connected { handshake: pre_hand, .. } => {
                            if let Some(pre_hand) = pre_hand {
                                if handshake.eq(&pre_hand.0) && pre_hand.2 == session {
                                    Ok(pre_hand.1.clone())
                                } else {
                                    log::warn!("[NeighbourConnection] Invalid handshake from {}, expected {:?}, got {:?}", self.remote, handshake, handshake);
                                    Err(NeighboursConnectError::InvalidData)
                                }
                            } else {
                                log::warn!("[NeighbourConnection] Invalid handshake from {}, expected {:?}, got None", self.remote, handshake);
                                Err(NeighboursConnectError::InvalidData)
                            }
                        }
                        _ => {
                            log::warn!("[NeighbourConnection] Invalid state, should be Connecting for connect request from {}", self.remote);
                            Err(NeighboursConnectError::InvalidState)
                        }
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
                                    self.output.push_back(Output::Event(ConnectionEvent::Connected(encryptor, decryptor)));
                                    self.state = State::Connected {
                                        last_pong_ms: now_ms,
                                        ping_seq: 0,
                                        stats: ConnectionStats { rtt_ms: INIT_RTT_MS },
                                        handshake: None,
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

#[cfg(test)]
mod tests {
    use crate::base::{MockDecryptor, MockEncryptor, MockHandshakeBuilder, MockHandshakeRequester, MockHandshakeResponder};

    use super::*;

    #[test]
    fn should_handle_outgoing_connect_correct() {
        let mut client_hanshake = MockHandshakeBuilder::default();
        client_hanshake.expect_requester().returning(move || {
            let mut requester = MockHandshakeRequester::default();
            requester.expect_create_public_request().return_once(|| Ok(vec![1, 2, 3]));
            requester
                .expect_process_public_response()
                .return_once(move |_| Ok((Box::new(MockEncryptor::default()), Box::new(MockDecryptor::default()))));
            Box::new(requester)
        });
        let remote: SocketAddr = "1.2.3.4:1000".parse().expect("Should parse");
        let mut client = NeighbourConnection::new_outgoing(Arc::new(client_hanshake), 1, 2, 1000, remote, 100);
        assert_eq!(
            client.pop_output(),
            Some(Output::Net(
                remote,
                NeighboursControlCmds::ConnectRequest {
                    to: 2,
                    session: 1000,
                    handshake: vec![1, 2, 3]
                }
            ))
        );

        //fake accepted
        client.on_input(
            1100,
            2,
            NeighboursControlCmds::ConnectResponse {
                session: 1000,
                result: Ok(vec![2, 3, 4]),
            },
        );
        assert_eq!(
            client.pop_output(),
            Some(Output::Event(ConnectionEvent::Connected(Box::new(MockEncryptor::default()), Box::new(MockDecryptor::default()))))
        );
    }

    #[test]
    fn should_handle_incoming_connect_correct() {
        let mut server_hanshake = MockHandshakeBuilder::default();
        server_hanshake.expect_responder().returning(move || {
            let mut responder = MockHandshakeResponder::default();
            responder
                .expect_process_public_request()
                .return_once(|req| Ok((Box::new(MockEncryptor::default()), Box::new(MockDecryptor::default()), req.to_vec())));
            Box::new(responder)
        });
        let remote: SocketAddr = "1.2.3.4:1000".parse().expect("Should parse");
        let mut server = NeighbourConnection::new_incoming(Arc::new(server_hanshake), 1, 2, 1000, remote, 100);
        server.on_input(
            1100,
            2,
            NeighboursControlCmds::ConnectRequest {
                to: 1,
                session: 1000,
                handshake: vec![1, 2, 3],
            },
        );

        assert_eq!(
            server.pop_output(),
            Some(Output::Event(ConnectionEvent::Connected(Box::new(MockEncryptor::default()), Box::new(MockDecryptor::default()))))
        );
        assert_eq!(
            server.pop_output(),
            Some(Output::Net(
                remote,
                NeighboursControlCmds::ConnectResponse {
                    session: 1000,
                    result: Ok(vec![1, 2, 3])
                }
            ))
        );
        assert_eq!(server.pop_output(), None);

        // should not response after Connected with wrong handshake
        server.on_input(
            1100,
            2,
            NeighboursControlCmds::ConnectRequest {
                to: 1,
                session: 1000,
                handshake: vec![1, 2, 3, 4],
            },
        );
        assert_eq!(
            server.pop_output(),
            Some(Output::Net(
                remote,
                NeighboursControlCmds::ConnectResponse {
                    session: 1000,
                    result: Err(NeighboursConnectError::InvalidData)
                }
            ))
        );
        assert_eq!(server.pop_output(), None);

        // should response after Connected with same session and handshake for better connectivity
        server.on_input(
            1100,
            2,
            NeighboursControlCmds::ConnectRequest {
                to: 1,
                session: 1000,
                handshake: vec![1, 2, 3],
            },
        );
        assert_eq!(
            server.pop_output(),
            Some(Output::Net(
                remote,
                NeighboursControlCmds::ConnectResponse {
                    session: 1000,
                    result: Ok(vec![1, 2, 3])
                }
            ))
        );
        assert_eq!(server.pop_output(), None);
    }
}

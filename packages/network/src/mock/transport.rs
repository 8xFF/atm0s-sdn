use crate::mock::connection_receiver::MockConnectionReceiver;
use crate::mock::connection_sender::MockConnectionSender;
use crate::mock::{MockInput, MockOutput};
use crate::transport::{AsyncConnectionAcceptor, ConnectionEvent, OutgoingConnectionError, Transport, TransportConnectingOutgoing, TransportConnector, TransportEvent};
use async_std::channel::{unbounded, Receiver, Sender};
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct MockTransportConnector {
    output: Arc<Mutex<VecDeque<MockOutput>>>,
    conn_id: Arc<AtomicU64>,
}

impl TransportConnector for MockTransportConnector {
    fn connect_to(&self, node_id: NodeId, dest: NodeAddr) -> Result<TransportConnectingOutgoing, OutgoingConnectionError> {
        let conn_seed = self.conn_id.fetch_add(1, Ordering::Relaxed);
        let conn_id = ConnId::from_out(0, conn_seed);
        self.output.lock().push_back(MockOutput::ConnectTo(node_id, dest));
        Ok(TransportConnectingOutgoing { conn_id })
    }
}

pub struct MockTransport {
    sender: Sender<MockInput>,
    receiver: Receiver<MockInput>,
    output: Arc<Mutex<VecDeque<MockOutput>>>,
    in_conns: HashMap<ConnId, Sender<Option<ConnectionEvent>>>,
    out_conns: HashMap<ConnId, Sender<Option<ConnectionEvent>>>,
    conn_id: Arc<AtomicU64>,
}

impl MockTransport {
    pub fn new() -> (Self, Sender<MockInput>, Arc<Mutex<VecDeque<MockOutput>>>) {
        let (sender, receiver) = unbounded();
        let output = Arc::new(Mutex::new(VecDeque::new()));
        (
            Self {
                sender: sender.clone(),
                receiver,
                output: output.clone(),
                in_conns: Default::default(),
                out_conns: Default::default(),
                conn_id: Default::default(),
            },
            sender,
            output,
        )
    }
}

#[async_trait::async_trait]
impl Transport for MockTransport {
    fn connector(&self) -> Arc<dyn TransportConnector> {
        Arc::new(MockTransportConnector {
            output: self.output.clone(),
            conn_id: self.conn_id.clone(),
        })
    }

    async fn recv(&mut self) -> Result<TransportEvent, ()> {
        loop {
            log::debug!("waiting mock transport event");
            let input = self.receiver.recv().await.map_err(|_e| ())?;
            match input {
                MockInput::FakeIncomingConnection(node, conn, addr) => {
                    let sender = self.sender.clone();
                    let (acceptor, acceptor_recv) = AsyncConnectionAcceptor::new();
                    async_std::task::spawn(async move {
                        match acceptor_recv.recv().await {
                            Ok(Ok(_)) => {
                                sender.send_blocking(MockInput::FakeIncomingConnectionForce(node, conn, addr)).unwrap();
                            }
                            Ok(Err(err)) => {
                                sender
                                    .send_blocking(MockInput::FakeOutgoingConnectionError(node, conn, OutgoingConnectionError::BehaviorRejected(err)))
                                    .unwrap();
                            }
                            _ => {
                                panic!("Must not happend");
                            }
                        }
                    });
                    break Ok(TransportEvent::IncomingRequest(node, conn, acceptor));
                }
                MockInput::FakeOutgoingConnection(node, conn, addr) => {
                    let sender = self.sender.clone();
                    let (acceptor, acceptor_recv) = AsyncConnectionAcceptor::new();
                    async_std::task::spawn(async move {
                        match acceptor_recv.recv().await {
                            Ok(Ok(_)) => {
                                sender.send_blocking(MockInput::FakeOutgoingConnectionForce(node, conn, addr)).unwrap();
                            }
                            _ => {}
                        }
                    });
                    break Ok(TransportEvent::OutgoingRequest(node, conn, acceptor));
                }
                MockInput::FakeIncomingConnectionForce(node, conn, addr) => {
                    log::debug!("FakeIncomingConnectionForce {} {} {}", node, conn, addr);
                    let (sender, receiver) = unbounded();
                    let conn_sender = MockConnectionSender {
                        remote_node_id: node,
                        conn_id: conn,
                        remote_addr: addr.clone(),
                        output: self.output.clone(),
                        internal_sender: sender.clone(),
                    };

                    let conn_recv = MockConnectionReceiver {
                        remote_node_id: node,
                        conn_id: conn,
                        remote_addr: addr.clone(),
                        receiver,
                    };

                    self.in_conns.insert(conn, sender);
                    break Ok(TransportEvent::Incoming(Arc::new(conn_sender), Box::new(conn_recv)));
                }
                MockInput::FakeOutgoingConnectionForce(node, conn, addr) => {
                    log::debug!("FakeOutgoingConnectionForce {} {} {}", node, conn, addr);
                    let (sender, receiver) = unbounded();
                    let conn_sender = MockConnectionSender {
                        remote_node_id: node,
                        conn_id: conn,
                        remote_addr: addr.clone(),
                        output: self.output.clone(),
                        internal_sender: sender.clone(),
                    };

                    let conn_recv = MockConnectionReceiver {
                        remote_node_id: node,
                        conn_id: conn,
                        remote_addr: addr.clone(),
                        receiver,
                    };

                    self.out_conns.insert(conn, sender);
                    break Ok(TransportEvent::Outgoing(Arc::new(conn_sender), Box::new(conn_recv)));
                }
                MockInput::FakeOutgoingConnectionError(node_id, connection_id, err) => {
                    self.out_conns.remove(&connection_id);
                    break Ok(TransportEvent::OutgoingError {
                        node_id: node_id,
                        conn_id: connection_id,
                        err,
                    });
                }
                MockInput::FakeIncomingMsg(conn, msg) => {
                    log::debug!("FakeIncomingMsg {} {}", msg.header.service_id, conn);
                    if let Some(sender) = self.in_conns.get(&conn) {
                        sender.send_blocking(Some(ConnectionEvent::Msg(msg))).unwrap();
                    } else if let Some(sender) = self.out_conns.get(&conn) {
                        sender.send_blocking(Some(ConnectionEvent::Msg(msg))).unwrap();
                    } else {
                        panic!("connection not found");
                    }
                }
                MockInput::FakeDisconnectIncoming(node_id, conn) => {
                    log::debug!("FakeDisconnectIncoming {} {}", node_id, conn);
                    if let Some(sender) = self.in_conns.remove(&conn) {
                        sender.send_blocking(None).unwrap();
                    } else {
                        panic!("connection not found");
                    }
                }
                MockInput::FakeDisconnectOutgoing(node_id, conn) => {
                    log::debug!("FakeDisconnectOutgoing {} {}", node_id, conn);
                    if let Some(sender) = self.out_conns.remove(&conn) {
                        sender.send_blocking(None).unwrap();
                    } else {
                        panic!("connection not found");
                    }
                }
            }
        }
    }
}

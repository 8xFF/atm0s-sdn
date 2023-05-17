use crate::mock::connection_receiver::MockConnectionReceiver;
use crate::mock::connection_sender::MockConnectionSender;
use crate::mock::{MockInput, MockOutput};
use crate::transport::{
    ConnectionEvent, ConnectionMsg, ConnectionSender, OutgoingConnectionError, Transport,
    TransportConnector, TransportEvent, TransportPendingOutgoing,
};
use kanal::{AsyncReceiver, AsyncSender, Sender, unbounded_async};
use bluesea_identity::{PeerAddr, PeerId};
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub struct MockTransportConnector<M: Send + Sync> {
    output: Arc<Mutex<VecDeque<MockOutput<M>>>>,
    conn_id: Arc<AtomicU32>,
}

impl<M: Send + Sync> TransportConnector for MockTransportConnector<M> {
    fn connect_to(
        &self,
        peer_id: PeerId,
        dest: PeerAddr,
    ) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        let conn_id = self.conn_id.fetch_add(1, Ordering::Relaxed);
        self.output
            .lock()
            .push_back(MockOutput::ConnectTo(peer_id, dest));
        Ok(TransportPendingOutgoing {
            connection_id: conn_id,
        })
    }
}

pub struct MockTransport<M> {
    receiver: AsyncReceiver<MockInput<M>>,
    output: Arc<Mutex<VecDeque<MockOutput<M>>>>,
    conns: HashMap<u32, Sender<Option<ConnectionEvent<M>>>>,
    conn_id: Arc<AtomicU32>,
}

impl<M> MockTransport<M> {
    pub fn new() -> (
        Self,
        AsyncSender<MockInput<M>>,
        Arc<Mutex<VecDeque<MockOutput<M>>>>,
    ) {
        let (sender, receiver) = unbounded_async();
        let output = Arc::new(Mutex::new(VecDeque::new()));
        (
            Self {
                receiver,
                output: output.clone(),
                conns: Default::default(),
                conn_id: Default::default(),
            },
            sender,
            output,
        )
    }
}

#[async_trait::async_trait]
impl<M: Send + Sync + 'static> Transport<M> for MockTransport<M> {
    fn connector(&self) -> Arc<dyn TransportConnector> {
        Arc::new(MockTransportConnector {
            output: self.output.clone(),
            conn_id: self.conn_id.clone(),
        })
    }

    async fn recv(&mut self) -> Result<TransportEvent<M>, ()> {
        loop {
            let input = self.receiver.recv().await.map_err(|e| ())?;
            match input {
                MockInput::FakeIncomingConnection(peer, conn, addr) => {
                    let (sender, receiver) = unbounded_async();
                    let conn_sender: MockConnectionSender<M> = MockConnectionSender {
                        remote_peer_id: peer,
                        conn_id: conn,
                        remote_addr: addr.clone(),
                        output: self.output.clone(),
                        internal_sender: sender.clone_sync(),
                    };

                    let conn_recv: MockConnectionReceiver<M> = MockConnectionReceiver {
                        remote_peer_id: peer,
                        conn_id: conn,
                        remote_addr: addr.clone(),
                        receiver,
                    };

                    self.conns.insert(conn, sender.to_sync());
                    break Ok(TransportEvent::Incoming(
                        Arc::new(conn_sender),
                        Box::new(conn_recv),
                    ));
                }
                MockInput::FakeIncomingMsg(service_id, conn, msg) => {
                    if let Some(sender) = self.conns.get(&conn) {
                        sender
                            .send(Some(ConnectionEvent::Msg { service_id, msg }))
                            .unwrap();
                    }
                }
                MockInput::FakeDisconnectIncoming(peer_id, conn) => {
                    if let Some(sender) = self.conns.get(&conn) {
                        sender.send(None).unwrap();
                    }
                }
            }
        }
    }
}

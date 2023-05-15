use std::collections::VecDeque;
use std::sync::Arc;
use async_std::channel::Sender;
use parking_lot::Mutex;
use bluesea_identity::{PeerAddr, PeerId};
use crate::mock::MockOutput;
use crate::transport::{ConnectionEvent, ConnectionMsg, ConnectionSender};

pub struct MockConnectionSender<MSG> {
    pub peer_id: PeerId,
    pub conn_id: u32,
    pub remote_addr: PeerAddr,
    pub output: Arc<Mutex<VecDeque<MockOutput<MSG>>>>,
}

impl<MSG> ConnectionSender<MSG> for MockConnectionSender<MSG> where MSG: Send + Sync {
    fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> PeerAddr {
        self.remote_addr.clone()
    }

    fn send(&self, service_id: u8, msg: ConnectionMsg<MSG>) {
        self.output.lock().push_back(MockOutput::SendTo(service_id, self.peer_id, self.conn_id, msg));
    }

    fn close(&self) {
        todo!()
    }
}
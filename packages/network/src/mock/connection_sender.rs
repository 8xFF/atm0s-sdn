use crate::mock::MockOutput;
use crate::transport::{ConnectionEvent, ConnectionSender};
use async_std::channel::Sender;
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;
use crate::msg::TransportMsg;

pub struct MockConnectionSender {
    pub(crate) remote_node_id: NodeId,
    pub(crate) conn_id: ConnId,
    pub(crate) remote_addr: NodeAddr,
    pub(crate) output: Arc<Mutex<VecDeque<MockOutput>>>,
    pub(crate) internal_sender: Sender<Option<ConnectionEvent>>,
}

impl ConnectionSender for MockConnectionSender {
    fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    fn remote_addr(&self) -> NodeAddr {
        self.remote_addr.clone()
    }

    fn send(&self, msg: TransportMsg) {
        self.output.lock().push_back(MockOutput::SendTo(self.remote_node_id, self.conn_id, msg));
    }

    fn close(&self) {
        self.internal_sender.send_blocking(None);
    }
}

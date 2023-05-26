use crate::mock::MockOutput;
use crate::transport::{ConnectionEvent, ConnectionMsg, ConnectionSender};
use async_std::channel::Sender;
use bluesea_identity::{NodeAddr, NodeId};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct MockConnectionSender<MSG> {
    pub(crate) remote_node_id: NodeId,
    pub(crate) conn_id: u32,
    pub(crate) remote_addr: NodeAddr,
    pub(crate) output: Arc<Mutex<VecDeque<MockOutput<MSG>>>>,
    pub(crate) internal_sender: Sender<Option<ConnectionEvent<MSG>>>,
}

impl<MSG> ConnectionSender<MSG> for MockConnectionSender<MSG>
where
    MSG: Send + Sync,
{
    fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> NodeAddr {
        self.remote_addr.clone()
    }

    fn send(&self, service_id: u8, msg: ConnectionMsg<MSG>) {
        self.output.lock().push_back(MockOutput::SendTo(
            service_id,
            self.remote_node_id,
            self.conn_id,
            msg,
        ));
    }

    fn close(&self) {
        self.internal_sender.send_blocking(None);
    }
}

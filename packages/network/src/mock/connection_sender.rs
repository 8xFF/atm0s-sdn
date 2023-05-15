use bluesea_identity::{PeerAddr, PeerId};
use crate::transport::ConnectionSender;

pub struct MockConnectionSender {
    peer_id: PeerId,
    conn_id: u32,
    remote_addr: PeerAddr,
}

impl ConnectionSender for MockConnectionSender {
    fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> PeerAddr {
        self.remote_addr.clone()
    }

    fn send_stream_reliable(&self, stream_id: u16, data: &[u8]) {
        todo!()
    }

    fn send_stream_unreliable(&self, stream_id: u16, data: &[u8]) {
        todo!()
    }

    fn close(&self) {
        todo!()
    }
}
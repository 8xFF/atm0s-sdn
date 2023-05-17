use bluesea_identity::PeerId;
use std::collections::HashMap;

#[derive(Default)]
pub struct ConnectionGrouping {
    peers: HashMap<PeerId, HashMap<u32, bool>>,
}

impl ConnectionGrouping {
    pub fn add(&mut self, peer: PeerId, conn_id: u32) -> bool {
        let entry = self.peers.entry(peer).or_insert_with(Default::default);
        let new = entry.is_empty();
        entry.insert(conn_id, true);
        new
    }

    pub fn remove(&mut self, peer: PeerId, conn_id: u32) -> bool {
        if let Some(conns) = self.peers.get_mut(&peer) {
            conns.remove(&conn_id);
            if conns.is_empty() {
                self.peers.remove(&peer);
                return true;
            }
        }
        false
    }
}

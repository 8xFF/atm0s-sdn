use bluesea_identity::{PeerAddr, PeerId};

enum PeerState {
    Waiting,
    Connecting,
    Sent,
}

pub struct ClosestList<M> {
    local_peer_id: PeerId,
    m: Vec<M>,
}

impl<M> Default for ClosestList<M> {
    fn default() -> Self {
        todo!()
    }
}

impl<M> ClosestList<M> {
    pub fn new() -> Self {
        todo!()
    }

    /// This will add peer to list, if is requested => state is Sent
    /// All added peer is sorted by Distance to local_peer_id
    pub fn add_peer(&mut self, peer: PeerId, addr: PeerAddr, requested: bool) {
        todo!()
    }

    /// This will pop closest peer which is Waiting state, if not exists it will return None
    pub fn pop_need_connect(&mut self) -> Option<(PeerId, PeerAddr)> {
        todo!()
    }

    /// This will add message to waiting list for a peer, this list is used when peer is connected
    pub fn add_pending_msg(&mut self, to: PeerId, msg: M) {
        todo!()
    }

    /// When a peer connected, this function return which message should be send
    pub fn pop_pending_msg(&mut self, to: PeerId) -> Option<M> {
        todo!()
    }
}

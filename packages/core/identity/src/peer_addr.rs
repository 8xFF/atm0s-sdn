use crate::peer_id::PeerId;
use parking_lot::Mutex;
pub type PeerAddr = multiaddr::Multiaddr;
pub use multiaddr::Protocol;

pub trait PeerAddrType {
    fn peer_id(&self) -> Option<PeerId>;
}

impl PeerAddrType for PeerAddr {
    fn peer_id(&self) -> Option<PeerId> {
        for protocol in self.iter() {
            match protocol {
                Protocol::P2p(node_id) => return Some(node_id),
                _ => {}
            }
        }
        None
    }
}

pub struct PeerAddrBuilder {
    addr: Mutex<PeerAddr>,
}

impl Default for PeerAddrBuilder {
    fn default() -> Self {
        Self {
            addr: Mutex::new(PeerAddr::empty()),
        }
    }
}

impl PeerAddrBuilder {
    pub fn add_protocol(&self, protocol: Protocol) {
        self.addr.lock().push(protocol);
    }

    pub fn addr(&self) -> PeerAddr {
        (*self.addr.lock()).clone()
    }
}

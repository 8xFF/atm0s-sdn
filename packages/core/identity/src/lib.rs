pub type PeerId = u32;
pub type PeerAddr = String;

pub enum PeerSegment {
    Public,
    Private(u8),
}

pub trait PeerIdType: Clone {
    fn random() -> PeerId;
    fn segment(&self) -> PeerSegment;
    fn distance(&self, other: &PeerId) -> u32;
}

impl PeerIdType for PeerId {
    fn random() -> PeerId {
        rand::random()
    }

    fn segment(&self) -> PeerSegment {
        PeerSegment::Public
    }

    fn distance(&self, other: &PeerId) -> u32 {
        self ^ *other
    }
}

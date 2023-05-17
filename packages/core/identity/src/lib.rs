pub type PeerId = u32;
pub type PeerAddr = multiaddr::Multiaddr;
pub use multiaddr;

pub enum PeerSegment {
    Public,
    Private(u8),
}

pub trait PeerIdType: Clone {
    fn random() -> PeerId;
    fn segment(&self) -> PeerSegment;
    fn distance(&self, other: &PeerId) -> u32;
    fn distance_bits(&self, other: &PeerId) -> u8;
    fn bucket_index(&self) -> u8;
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

    fn distance_bits(&self, other: &PeerId) -> u8 {
        let distance = self.distance(other);
        (32 - distance.leading_zeros()) as u8
    }

    fn bucket_index(&self) -> u8 {
        (32 - self.leading_zeros()) as u8
    }
}

#[cfg(test)]
mod tests {
    use crate::{PeerId, PeerIdType};

    #[test]
    fn my_distance() {
        let id1: PeerId = 30;
        assert_eq!(id1.distance(&id1), 0);
        assert_eq!(id1.distance_bits(&id1), 0);
    }

    #[test]
    fn simple_distance() {
        let id1: PeerId = 0;
        assert_eq!(id1.distance_bits(&0), 0);
        assert_eq!(id1.distance_bits(&1), 1);
        assert_eq!(id1.distance_bits(&2), 2);
        assert_eq!(id1.distance_bits(&3), 2);
        assert_eq!(id1.distance_bits(&u32::MAX), 32);
    }
}

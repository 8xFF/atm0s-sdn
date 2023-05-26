pub type NodeId = u32;

pub enum NodeSegment {
    Public,
    Private(u8),
}

pub trait NodeIdType: Clone {
    fn random() -> NodeId;
    fn segment(&self) -> NodeSegment;
    fn distance(&self, other: &NodeId) -> u32;
    fn distance_bits(&self, other: &NodeId) -> u8;
    fn bucket_index(&self) -> u8;
}

impl NodeIdType for NodeId {
    fn random() -> NodeId {
        rand::random()
    }

    fn segment(&self) -> NodeSegment {
        NodeSegment::Public
    }

    fn distance(&self, other: &NodeId) -> u32 {
        self ^ *other
    }

    fn distance_bits(&self, other: &NodeId) -> u8 {
        let distance = self.distance(other);
        (32 - distance.leading_zeros()) as u8
    }

    fn bucket_index(&self) -> u8 {
        (32 - self.leading_zeros()) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeId, NodeIdType};

    #[test]
    fn my_distance() {
        let id1: NodeId = 30;
        assert_eq!(id1.distance(&id1), 0);
        assert_eq!(id1.distance_bits(&id1), 0);
    }

    #[test]
    fn simple_distance() {
        let id1: NodeId = 0;
        assert_eq!(id1.distance_bits(&0), 0);
        assert_eq!(id1.distance_bits(&1), 1);
        assert_eq!(id1.distance_bits(&2), 2);
        assert_eq!(id1.distance_bits(&3), 2);
        assert_eq!(id1.distance_bits(&u32::MAX), 32);
    }
}

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

    fn build(geo1: u8, geo2: u8, group: u8, index: u8) -> Self;
    fn build2(zone_id: u16, group: u8, index: u8) -> Self;
    fn layer(&self, index: u8) -> u8;
    fn geo1(&self) -> u8;
    fn geo2(&self) -> u8;
    fn group(&self) -> u8;
    fn index(&self) -> u8;
    fn eq_util_layer(&self, other: &Self) -> u8;
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

    fn layer(&self, index: u8) -> u8 {
        assert!(index <= 3);

        (*self >> 8 * index) as u8
    }

    fn build(geo1: u8, geo2: u8, group: u8, index: u8) -> Self {
        ((geo1 as u32) << 8 * 3) | ((geo2 as u32) << 8 * 2) | ((group as u32) << 8 * 1) | (index as u32)
    }

    fn build2(zone_id: u16, group: u8, index: u8) -> Self {
        ((zone_id as u32) << 16) | ((group as u32) << 8 * 1) | (index as u32)
    }

    fn geo1(&self) -> u8 {
        (*self >> 8 * 3) as u8
    }

    fn geo2(&self) -> u8 {
        (*self >> 8 * 2) as u8
    }

    fn group(&self) -> u8 {
        (*self >> 8 * 1) as u8
    }

    fn index(&self) -> u8 {
        *self as u8
    }

    fn eq_util_layer(&self, other: &Self) -> u8 {
        if self.layer(3) != other.layer(3) {
            return 4
        }

        if self.layer(2) != other.layer(2) {
            return 3
        }

        if self.layer(1) != other.layer(1) {
            return 2
        }

        if self.layer(0) != other.layer(0) {
            return 1
        }

        return 0
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

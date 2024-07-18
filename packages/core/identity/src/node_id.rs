pub type NodeId = u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Enum representing a segment of a node ID.
pub enum NodeSegment {
    Public,
    Private(u8),
}

pub trait NodeIdType: Clone {
    /// Generates a random `NodeId`.
    fn random() -> NodeId;
    /// Returns the segment of the node ID.
    fn segment(&self) -> NodeSegment;
    /// Calculates the distance between two NodeIds.
    fn distance(&self, other: &NodeId) -> u32;
    /// Calculates the distance in bits between two NodeIds.
    ///
    /// # Arguments
    ///
    /// * `other` - The other NodeId to calculate the distance to.
    ///
    /// # Returns
    ///
    /// The distance in bits between the two NodeIds.
    fn distance_bits(&self, other: &NodeId) -> u8;
    /// Returns the index of the bucket that this node ID belongs to.
    fn bucket_index(&self) -> u8;

    /// Builds a new `NodeId` with the given geographic location, group, and index.
    ///
    /// # Arguments
    ///
    /// * `geo1` - The first geographic location byte.
    /// * `geo2` - The second geographic location byte.
    /// * `group` - The group byte.
    /// * `index` - The index byte.
    fn build(geo1: u8, geo2: u8, group: u8, index: u8) -> Self;
    /// Builds a `NodeId` from a zone ID, group, and index.
    fn build2(zone_id: u16, group: u8, index: u8) -> Self;
    /// Returns the value of the layer of the node at the given index.
    fn layer(&self, index: u8) -> u8;
    /// Returns the first geographic location byte.
    fn geo1(&self) -> u8;
    /// Returns the second geographic location byte.
    fn geo2(&self) -> u8;
    /// Returns the group byte.
    fn group(&self) -> u8;
    /// Returns the index byte.
    fn index(&self) -> u8;
    /// Returns the number of layers that the two NodeIds are equal up to.
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

        (*self >> (8 * index)) as u8
    }

    fn build(geo1: u8, geo2: u8, group: u8, index: u8) -> Self {
        ((geo1 as u32) << (8 * 3)) | ((geo2 as u32) << (8 * 2)) | ((group as u32) << 8) | (index as u32)
    }

    fn build2(zone_id: u16, group: u8, index: u8) -> Self {
        ((zone_id as u32) << 16) | ((group as u32) << 8) | (index as u32)
    }

    fn geo1(&self) -> u8 {
        (*self >> (8 * 3)) as u8
    }

    fn geo2(&self) -> u8 {
        (*self >> (8 * 2)) as u8
    }

    fn group(&self) -> u8 {
        (*self >> 8) as u8
    }

    fn index(&self) -> u8 {
        *self as u8
    }

    // TODO: typo in name, should be eq_until_layer
    fn eq_util_layer(&self, other: &Self) -> u8 {
        if self.layer(3) != other.layer(3) {
            return 4;
        }

        if self.layer(2) != other.layer(2) {
            return 3;
        }

        if self.layer(1) != other.layer(1) {
            return 2;
        }

        if self.layer(0) != other.layer(0) {
            return 1;
        }

        0
    }
}

#[cfg(test)]
mod tests {
    use crate::NodeSegment;

    use super::{NodeId, NodeIdType};

    #[test]
    fn my_distance() {
        let id1: NodeId = 30;
        assert_eq!(id1.distance(&id1), 0);
        assert_eq!(id1.distance_bits(&id1), 0);
    }

    #[test]
    fn test_random() {
        let id1 = NodeId::random();
        let id2 = NodeId::random();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_segment() {
        let id1: NodeId = 30;
        assert_eq!(id1.segment(), NodeSegment::Public);
    }

    #[test]
    fn test_distance() {
        let id1: NodeId = 0;
        let id2: NodeId = 1;
        let id3: NodeId = 2;
        let id4: NodeId = 3;
        let id5: NodeId = u32::MAX;

        assert_eq!(id1.distance(&id1), 0);
        assert_eq!(id1.distance_bits(&id1), 0);

        assert_eq!(id1.distance_bits(&id2), 1);
        assert_eq!(id1.distance_bits(&id3), 2);
        assert_eq!(id1.distance_bits(&id4), 2);
        assert_eq!(id1.distance_bits(&id5), 32);
    }

    #[test]
    fn test_bucket_index() {
        let id1: NodeId = 0;
        let id2: NodeId = 1;
        let id3: NodeId = 2;
        let id4: NodeId = 3;
        let id5: NodeId = u32::MAX;

        assert_eq!(id1.bucket_index(), 0);
        assert_eq!(id2.bucket_index(), 1);
        assert_eq!(id3.bucket_index(), 2);
        assert_eq!(id4.bucket_index(), 2);
        assert_eq!(id5.bucket_index(), 32);
    }

    #[test]
    fn test_layer() {
        let id1: NodeId = 0x12345678;
        assert_eq!(id1.layer(0), 0x78);
        assert_eq!(id1.layer(1), 0x56);
        assert_eq!(id1.layer(2), 0x34);
        assert_eq!(id1.layer(3), 0x12);
    }

    #[test]
    fn test_eq_util_layer() {
        let id1: NodeId = 0x12345678;
        let id2: NodeId = 0x12345679;
        let id3: NodeId = 0x12345778;
        let id4: NodeId = 0x12355678;
        let id5: NodeId = 0x12345678;

        assert_eq!(id1.eq_util_layer(&id2), 1);
        assert_eq!(id1.eq_util_layer(&id3), 2);
        assert_eq!(id1.eq_util_layer(&id4), 3);
        assert_eq!(id1.eq_util_layer(&id5), 0);
    }
}

use crate::kbucket::bucket::KBucket;
use crate::kbucket::entry::EntryState;
use bluesea_identity::{NodeAddr, NodeId, NodeIdType};

pub mod bucket;
pub mod entry;

pub(crate) const KEY_BITS: usize = 32;
pub(crate) const K_BUCKET: usize = 4;

struct KBucketTable {
    buckets: [KBucket; KEY_BITS + 1],
}

#[allow(dead_code)]
impl KBucketTable {
    pub fn new() -> Self {
        Self {
            buckets: [
                KBucket::new(0),
                KBucket::new(1),
                KBucket::new(2),
                KBucket::new(3),
                KBucket::new(4),
                KBucket::new(5),
                KBucket::new(6),
                KBucket::new(7),
                KBucket::new(8),
                KBucket::new(9),
                KBucket::new(10),
                KBucket::new(11),
                KBucket::new(12),
                KBucket::new(13),
                KBucket::new(14),
                KBucket::new(15),
                KBucket::new(16),
                KBucket::new(17),
                KBucket::new(18),
                KBucket::new(19),
                KBucket::new(20),
                KBucket::new(21),
                KBucket::new(22),
                KBucket::new(23),
                KBucket::new(24),
                KBucket::new(25),
                KBucket::new(26),
                KBucket::new(27),
                KBucket::new(28),
                KBucket::new(29),
                KBucket::new(30),
                KBucket::new(31),
                KBucket::new(32),
            ],
        }
    }

    pub fn size(&self) -> usize {
        let mut sum = 0;
        for b in &self.buckets {
            sum += b.size() as usize;
        }
        sum
    }

    pub fn connected_size(&self) -> usize {
        let mut sum = 0;
        for b in &self.buckets {
            sum += b.connected_size() as usize;
        }
        sum
    }

    pub fn get_node(&self, distance: NodeId) -> Option<&EntryState> {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &self.buckets[bucket_index as usize];
        bucket.get_node(distance)
    }

    pub fn add_node_connecting(&mut self, distance: NodeId, addr: NodeAddr) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.add_node_connecting(distance, addr)
    }

    pub fn add_node_connected(&mut self, distance: NodeId, addr: NodeAddr) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.add_node_connected(distance, addr)
    }

    pub fn remove_connecting_node(&mut self, distance: NodeId) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.remove_connecting_node(distance)
    }

    pub fn remove_connected_node(&mut self, distance: NodeId) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.remove_connected_node(distance)
    }

    pub fn remove_timeout_nodes(&mut self) -> Vec<NodeId> {
        //TODO concat results
        for bucket in &mut self.buckets {
            bucket.remove_timeout_nodes();
        }
        vec![]
    }

    pub fn closest_nodes(&self, distance: NodeId) -> Vec<(NodeId, NodeAddr, bool)> {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let mut closest_node: Vec<(NodeId, NodeAddr, bool)> = self.buckets[bucket_index as usize].nodes();
        let need_more = closest_node.len() < K_BUCKET;
        if need_more && bucket_index > 1 {
            let mut more_bucket_index = bucket_index - 1;
            //TODO reduce number of loop, maybe iter with 1 in key binary
            while more_bucket_index > 0 {
                let new_list: Vec<(NodeId, NodeAddr, bool)> = self.buckets[more_bucket_index as usize].nodes();
                for part in new_list {
                    closest_node.push(part);
                }
                more_bucket_index -= 1;
            }
        }
        if need_more && bucket_index < KEY_BITS as u8 {
            let mut more_bucket_index = bucket_index + 1;
            //TODO reduce number of loop, maybe iter with 1 in key binary
            while more_bucket_index <= KEY_BITS as u8 {
                let new_list: Vec<(NodeId, NodeAddr, bool)> = self.buckets[more_bucket_index as usize].nodes();
                for part in new_list {
                    closest_node.push(part);
                }
                more_bucket_index += 1;
            }
        }
        closest_node.sort_by_key(|(node, _, _)| distance ^ *node);
        closest_node.truncate(K_BUCKET);
        closest_node
    }
}

pub struct KBucketTableWrap {
    local_node_id: NodeId,
    table: KBucketTable,
}

#[allow(dead_code)]
impl KBucketTableWrap {
    pub fn new(local_node_id: NodeId) -> Self {
        Self {
            local_node_id,
            table: KBucketTable::new(),
        }
    }

    pub fn size(&self) -> usize {
        self.table.size()
    }

    pub fn connected_size(&self) -> usize {
        self.table.connected_size()
    }

    pub fn get_node(&self, node: NodeId) -> Option<&EntryState> {
        self.table.get_node(node)
    }

    pub fn add_node_connecting(&mut self, node: NodeId, addr: NodeAddr) -> bool {
        self.table.add_node_connecting(node ^ self.local_node_id, addr)
    }

    pub fn add_node_connected(&mut self, node: NodeId, addr: NodeAddr) -> bool {
        self.table.add_node_connected(node ^ self.local_node_id, addr)
    }

    pub fn remove_connecting_node(&mut self, node: NodeId) -> bool {
        self.table.remove_connecting_node(node ^ self.local_node_id)
    }

    pub fn remove_connected_node(&mut self, node: NodeId) -> bool {
        self.table.remove_connected_node(node ^ self.local_node_id)
    }

    pub fn remove_timeout_nodes(&mut self) -> Vec<NodeId> {
        let mut removed = self.table.remove_timeout_nodes();
        for key in &mut removed {
            *key ^= self.local_node_id
        }
        removed
    }

    pub fn closest_nodes(&self, key: NodeId) -> Vec<(NodeId, NodeAddr, bool)> {
        let mut closest = self.table.closest_nodes(key ^ self.local_node_id);
        for (key, _, _) in &mut closest {
            *key ^= self.local_node_id
        }
        closest
    }
}

#[cfg(test)]
mod tests {
    use crate::kbucket::{KBucketTable, KBucketTableWrap};
    use bluesea_identity::{NodeAddr, Protocol};

    #[test]
    fn simple_table() {
        let mut table = KBucketTable::new();
        assert_eq!(table.add_node_connecting(1, NodeAddr::from(Protocol::Udp(1))), true);
        assert_eq!(table.add_node_connecting(10, NodeAddr::from(Protocol::Udp(10))), true);
        assert_eq!(table.add_node_connecting(40, NodeAddr::from(Protocol::Udp(40))), true);
        assert_eq!(table.add_node_connecting(100, NodeAddr::from(Protocol::Udp(100))), true);
        assert_eq!(table.add_node_connecting(120, NodeAddr::from(Protocol::Udp(120))), true);
        assert_eq!(table.add_node_connecting(u32::MAX, NodeAddr::from(Protocol::Udp(5000))), true);

        assert_eq!(
            table.closest_nodes(100),
            vec![
                (100, NodeAddr::from(Protocol::Udp(100)), false),
                (120, NodeAddr::from(Protocol::Udp(120)), false),
                (40, NodeAddr::from(Protocol::Udp(40)), false),
                (1, NodeAddr::from(Protocol::Udp(1)), false)
            ]
        );

        assert_eq!(
            table.closest_nodes(10),
            vec![
                (10, NodeAddr::from(Protocol::Udp(10)), false),
                (1, NodeAddr::from(Protocol::Udp(1)), false),
                (40, NodeAddr::from(Protocol::Udp(40)), false),
                (100, NodeAddr::from(Protocol::Udp(100)), false),
            ]
        );

        assert_eq!(
            table.closest_nodes(u32::MAX),
            vec![
                (u32::MAX, NodeAddr::from(Protocol::Udp(5000)), false),
                (120, NodeAddr::from(Protocol::Udp(120)), false),
                (100, NodeAddr::from(Protocol::Udp(100)), false),
                (40, NodeAddr::from(Protocol::Udp(40)), false),
            ]
        );
    }

    fn random_insert(loop_count: usize, table_size: usize, test_count: usize) {
        for _ in 0..loop_count {
            let mut list = vec![];
            let mut table = KBucketTable::new();
            for _ in 0..table_size {
                let dis: u32 = rand::random();
                let addr = NodeAddr::from(Protocol::Udp(dis as u16));
                if table.add_node_connected(dis, addr.clone()) {
                    list.push((dis, addr, true));
                }
            }

            for _ in 0..test_count {
                let key: u32 = rand::random();
                let closest_nodes = table.closest_nodes(key);
                list.sort_by_key(|(dist, _, _)| *dist ^ key);
                assert_eq!(closest_nodes, list[0..closest_nodes.len()]);
            }
        }
    }

    fn test_manual(nodes: Vec<u32>, key: u32) {
        let mut list = vec![];
        let mut table = KBucketTable::new();
        for dis in nodes {
            let addr = NodeAddr::from(Protocol::Udp(dis as u16));
            if table.add_node_connected(dis, addr.clone()) {
                list.push((dis, addr, true));
            }
        }

        let closest_nodes = table.closest_nodes(key);
        list.sort_by_key(|(dist, _, _)| *dist ^ key);
        assert_eq!(closest_nodes, list[0..closest_nodes.len()]);
    }

    #[test]
    fn failed_1() {
        test_manual(vec![483704965, 473524180, 526503063, 190210392, 27511667], 299570928);
    }

    #[test]
    fn random_10() {
        random_insert(100, 10, 100);
    }

    #[test]
    fn random_50() {
        random_insert(100, 50, 500);
    }

    #[test]
    fn random_100() {
        random_insert(100, 100, 1000);
    }

    #[test]
    fn random_200() {
        random_insert(100, 200, 2000);
    }

    fn random_insert_wrap(loop_count: usize, table_size: usize, test_count: usize) {
        for _ in 0..loop_count {
            let local_node_id = rand::random();
            let mut list = vec![];
            let mut table = KBucketTableWrap::new(local_node_id);
            for _ in 0..table_size {
                let dis: u32 = rand::random();
                let addr = NodeAddr::from(Protocol::Udp(dis as u16));
                if table.add_node_connected(dis, addr.clone()) {
                    list.push((dis, addr, true));
                }
            }

            for _ in 0..test_count {
                let key: u32 = rand::random();
                let closest_nodes = table.closest_nodes(key);
                list.sort_by_key(|(dist, _, _)| *dist ^ key);
                assert_eq!(closest_nodes, list[0..closest_nodes.len()]);
            }
        }
    }

    #[test]
    fn random_10_wrap() {
        random_insert_wrap(100, 10, 100);
    }

    #[test]
    fn random_50_wrap() {
        random_insert_wrap(100, 50, 500);
    }

    #[test]
    fn random_100_wrap() {
        random_insert_wrap(100, 100, 1000);
    }

    #[test]
    fn random_200_wrap() {
        random_insert_wrap(100, 200, 2000);
    }
}

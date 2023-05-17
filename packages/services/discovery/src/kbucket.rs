use crate::kbucket::bucket::KBucket;
use crate::kbucket::entry::EntryState;
use bluesea_identity::{PeerAddr, PeerId, PeerIdType};

pub mod bucket;
pub mod entry;

pub(crate) const KEY_BITS: usize = 32;
pub(crate) const K_BUCKET: usize = 4;

struct KBucketTable {
    buckets: [KBucket; KEY_BITS + 1],
}

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

    pub fn get_peer(&self, distance: PeerId) -> Option<&EntryState> {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &self.buckets[bucket_index as usize];
        bucket.get_peer(distance)
    }

    pub fn add_peer_connecting(&mut self, distance: PeerId, addr: PeerAddr) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.add_peer_connecting(distance, addr)
    }

    pub fn add_peer_connected(&mut self, distance: PeerId, addr: PeerAddr) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.add_peer_connected(distance, addr)
    }

    pub fn remove_connecting_peer(&mut self, distance: PeerId) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.remove_connecting_peer(distance)
    }

    pub fn remove_connected_peer(&mut self, distance: PeerId) -> bool {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let bucket = &mut self.buckets[bucket_index as usize];
        bucket.remove_connected_peer(distance)
    }

    pub fn remove_timeout_peers(&mut self) -> Option<Vec<PeerId>> {
        //TODO concat results
        for bucket in &mut self.buckets {
            bucket.remove_timeout_peers();
        }
        None
    }

    pub fn closest_peers(&self, distance: PeerId) -> Vec<(PeerId, PeerAddr, bool)> {
        let bucket_index = distance.bucket_index();
        assert!(bucket_index <= KEY_BITS as u8);
        let mut closest_peer: Vec<(PeerId, PeerAddr, bool)> =
            self.buckets[bucket_index as usize].peers();
        if bucket_index > 1 {
            let mut more_bucket_index = bucket_index - 1;
            while closest_peer.len() < K_BUCKET && more_bucket_index > 0 {
                let new_list: Vec<(PeerId, PeerAddr, bool)> =
                    self.buckets[more_bucket_index as usize].peers();
                for part in new_list {
                    closest_peer.push(part);
                }
                more_bucket_index -= 1;
            }
        }
        if bucket_index < KEY_BITS as u8 {
            let mut more_bucket_index = bucket_index + 1;
            while closest_peer.len() < K_BUCKET && more_bucket_index <= KEY_BITS as u8 {
                let new_list: Vec<(PeerId, PeerAddr, bool)> =
                    self.buckets[more_bucket_index as usize].peers();
                for part in new_list {
                    closest_peer.push(part);
                }
                more_bucket_index += 1;
            }
        }
        closest_peer.sort_by_key(|(peer, _, _)| distance ^ *peer);
        closest_peer.truncate(K_BUCKET);
        closest_peer
    }
}

pub struct KBucketTableWrap {
    local_peer_id: PeerId,
    table: KBucketTable,
}

impl KBucketTableWrap {
    pub fn new(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            table: KBucketTable::new(),
        }
    }

    pub fn get_peer(&self, peer: PeerId) -> Option<&EntryState> {
        self.table.get_peer(peer)
    }

    pub fn add_peer_connecting(&mut self, peer: PeerId, addr: PeerAddr) -> bool {
        self.table
            .add_peer_connecting(peer ^ self.local_peer_id, addr)
    }

    pub fn add_peer_connected(&mut self, peer: PeerId, addr: PeerAddr) -> bool {
        self.table
            .add_peer_connected(peer ^ self.local_peer_id, addr)
    }

    pub fn remove_connecting_peer(&mut self, peer: PeerId) -> bool {
        self.table.remove_connecting_peer(peer ^ self.local_peer_id)
    }

    pub fn remove_connected_peer(&mut self, peer: PeerId) -> bool {
        self.table.remove_connected_peer(peer ^ self.local_peer_id)
    }

    pub fn remove_timeout_peers(&mut self) -> Vec<PeerId> {
        let mut removed = self.remove_timeout_peers();
        for key in &mut removed {
            *key = *key ^ self.local_peer_id
        }
        removed
    }

    pub fn closest_peers(&self, key: PeerId) -> Vec<(PeerId, PeerAddr, bool)> {
        let mut closest = self.table.closest_peers(key ^ self.local_peer_id);
        for (key, _, _) in &mut closest {
            *key = *key ^ self.local_peer_id
        }
        closest
    }
}

#[cfg(test)]
mod tests {
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::PeerAddr;
    use crate::kbucket::KBucketTable;

    #[test]
    fn simple_table() {
        let mut table = KBucketTable::new();
        assert_eq!(table.add_peer_connecting(1, PeerAddr::from(Protocol::Udp(1))), true);
        assert_eq!(table.add_peer_connecting(10, PeerAddr::from(Protocol::Udp(10))), true);
        assert_eq!(table.add_peer_connecting(40, PeerAddr::from(Protocol::Udp(40))), true);
        assert_eq!(table.add_peer_connecting(100, PeerAddr::from(Protocol::Udp(100))), true);
        assert_eq!(table.add_peer_connecting(120, PeerAddr::from(Protocol::Udp(120))), true);
        assert_eq!(
            table.add_peer_connecting(u32::MAX, PeerAddr::from(Protocol::Udp(5000))),
            true
        );

        assert_eq!(
            table.closest_peers(100),
            vec![
                (100, PeerAddr::from(Protocol::Udp(100)), false),
                (120, PeerAddr::from(Protocol::Udp(120)), false),
                (40, PeerAddr::from(Protocol::Udp(40)), false),
                (10, PeerAddr::from(Protocol::Udp(10)), false)
            ]
        );

        assert_eq!(
            table.closest_peers(10),
            vec![
                (10, PeerAddr::from(Protocol::Udp(10)), false),
                (1, PeerAddr::from(Protocol::Udp(1)), false),
                (40, PeerAddr::from(Protocol::Udp(40)), false),
                (100, PeerAddr::from(Protocol::Udp(100)), false),
            ]
        );

        assert_eq!(
            table.closest_peers(u32::MAX),
            vec![
                (u32::MAX, PeerAddr::from(Protocol::Udp(5000)), false),
                (120, PeerAddr::from(Protocol::Udp(120)), false),
                (100, PeerAddr::from(Protocol::Udp(100)), false),
                (40, PeerAddr::from(Protocol::Udp(40)), false),
            ]
        );
    }

    fn random_insert(table_size: usize, test_count: usize) {
        let mut list = vec![];
        let mut table = KBucketTable::new();
        for _ in 0..table_size {
            let dis: u32 = rand::random();
            let addr = PeerAddr::from(Protocol::Udp(dis as u16));
            if table.add_peer_connected(dis, addr.clone()) {
                list.push((dis, addr, true));
            }
        }

        for _ in 0..test_count {
            let key: u32 = rand::random();
            let closest_peers = table.closest_peers(key);
            list.sort_by_key(|(dist, _, _)| *dist ^ key);
            assert_eq!(closest_peers, list[0..closest_peers.len()]);
        }

        //299570928

    }

    fn test_manual(peers: Vec<u32>, key: u32) {
        let mut list = vec![];
        let mut table = KBucketTable::new();
        for dis in peers {
            let addr = PeerAddr::from(Protocol::Udp(dis as u16));
            if table.add_peer_connected(dis, addr.clone()) {
                list.push((dis, addr, true));
            }
        }

        let closest_peers = table.closest_peers(key);
        list.sort_by_key(|(dist, _, _)| *dist ^ key);
        assert_eq!(closest_peers, list[0..closest_peers.len()]);
    }

    #[test]
    fn failed_1() {
        test_manual(vec![483704965, 473524180, 526503063, 190210392, 27511667], 299570928);
    }

    #[test]
    fn random_100() {
        random_insert(100, 100);
    }

    #[test]
    fn random_1000() {
        random_insert(1000, 100);
    }

    #[test]
    fn random_10000() {
        random_insert(10000, 100);
    }
}

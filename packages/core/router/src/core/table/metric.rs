use std::cmp::Ordering;

use atm0s_sdn_identity::NodeId;
use serde::{Deserialize, Serialize};

pub const BANDWIDTH_LIMIT: u32 = 10000; //10Mbps
const BANDWIDTH_SCORE_PENALTY: u32 = 1000; //1s
const HOP_PLUS_RTT: u16 = 10; //10ms each hops
const LOCAL_BANDWIDTH: u32 = 1_000_000; //1Gbps

/// Concatenate two hops array, with condition that the last hop of `a` is the first hop of `b`, if not return None
pub fn concat_hops(a: &[NodeId], b: &[NodeId]) -> Vec<NodeId> {
    let mut ret = a.to_vec();
    ret.extend_from_slice(&b[0..]);
    ret
}

/// Path to destination, all nodes in reverse path
/// Example with local connection : A -> A => hops: [A],
/// Example with direct connection : A -> B => hops: [B, A],
/// Example with indirect connection : A -> B -> C => hops: [C, B, B],
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metric {
    latency: u16,      //in milliseconds
    hops: Vec<NodeId>, //in hops, from 0 (direct)
    bandwidth: u32,    //in kbps
                       // pub lost: f32,
                       // pub jitter: u16,
}

impl Metric {
    pub fn local() -> Self {
        Metric::new(0, vec![], LOCAL_BANDWIDTH)
    }

    pub fn direct(latency: u16, node: NodeId, bandwidth: u32) -> Self {
        Metric::new(latency, vec![node], bandwidth)
    }

    pub fn new(latency: u16, hops: Vec<NodeId>, bandwidth: u32) -> Self {
        Metric { latency, hops, bandwidth }
    }

    pub fn contain_in_hops(&self, node_id: NodeId) -> bool {
        self.hops.contains(&node_id)
    }

    pub fn add(&self, other: &Self) -> Self {
        Metric {
            latency: self.latency + other.latency,
            hops: concat_hops(&self.hops, &other.hops),
            bandwidth: std::cmp::min(self.bandwidth, other.bandwidth),
        }
    }

    pub fn score(&self) -> u32 {
        let based_score = self.latency as u32 + (self.hops.len() as u32 * HOP_PLUS_RTT as u32);
        if self.bandwidth >= BANDWIDTH_LIMIT {
            based_score
        } else {
            based_score + BANDWIDTH_SCORE_PENALTY
        }
    }

    /// Get destination of this metric, in case it is localy, it will be None
    pub fn dest_node(&self) -> Option<NodeId> {
        self.hops.first().cloned()
    }

    pub fn hops(&self) -> &[NodeId] {
        &self.hops
    }
}

impl Ord for Metric {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score().cmp(&other.score())
    }
}

impl Eq for Metric {}

impl PartialOrd for Metric {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq<Self> for Metric {
    fn eq(&self, other: &Self) -> bool {
        self.score() == other.score()
    }
}

#[cfg(test)]
mod tests {
    use super::Metric;

    #[test]
    fn eq() {
        let m1 = Metric::new(1, vec![1], 10000);
        let m2 = Metric::new(1, vec![1], 10000);

        assert_eq!(m1, m2);
    }

    #[test]
    fn compare() {
        let m1 = Metric::new(1, vec![1], 10000);
        let m2 = Metric::new(2, vec![2], 10000);
        let m3 = Metric::new(2, vec![3, 4], 10000);

        assert!(m1 < m2);
        assert!(m2 > m1);

        assert!(m1 < m3);
        assert!(m2 < m3);
        assert!(m3 > m2);
    }

    #[test]
    fn compare_bandwidth_limit() {
        let m1 = Metric::new(1, vec![1], 9000);
        let m2 = Metric::new(2, vec![2], 10000);
        let m3 = Metric::new(2, vec![3], 9000);
        let m4 = Metric::new(2, vec![3], 11000);

        assert!(m2 < m1);
        assert!(m1 > m2);
        assert!(m1 < m3);
        assert!(m3 > m1);

        assert!(m2 == m4);
    }

    #[test]
    fn add() {
        let m1 = Metric::new(1, vec![1, 2], 10000);
        let m2 = Metric::new(2, vec![3], 20000);
        assert_eq!(m1.add(&m2), Metric::new(3, vec![1, 2, 3], 10000));
    }

    #[test]
    fn hops_has_affect_latancy() {
        let m1 = Metric::new(1, vec![1, 2], 10000);
        let m2 = Metric::new(2, vec![2], 10000);

        assert!(m1 > m2);
        assert!(m2 < m1);

        let m3 = Metric::new(10, vec![1, 2], 10000);
        let m4 = Metric::new(12, vec![1], 10000);

        assert!(m3 > m4);
        assert!(m4 < m3);
    }
}

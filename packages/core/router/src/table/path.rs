use bluesea_identity::{ConnId, NodeId};
use std::cmp::Ordering;

use crate::table::Metric;

#[derive(Debug, Clone)]
pub struct Path(pub ConnId, pub NodeId, pub Metric);

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.2.partial_cmp(&other.2)
    }
}

impl PartialEq<Self> for Path {
    fn eq(&self, other: &Self) -> bool {
        self.2.eq(&other.2)
    }
}

impl Eq for Path {}

impl Ord for Path {
    fn cmp(&self, other: &Self) -> Ordering {
        self.2.partial_cmp(&other.2).unwrap()
    }
}

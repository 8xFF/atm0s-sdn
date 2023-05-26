use bluesea_identity::NodeId;
use std::cmp::Ordering;

use crate::table::Metric;

#[derive(Debug, Clone)]
pub struct Path(pub NodeId, pub Metric);

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.1.partial_cmp(&other.1)
    }
}

impl PartialEq<Self> for Path {
    fn eq(&self, other: &Self) -> bool {
        self.1.eq(&other.1)
    }
}

impl Eq for Path {}

impl Ord for Path {
    fn cmp(&self, other: &Self) -> Ordering {
        self.1.partial_cmp(&other.1).unwrap()
    }
}

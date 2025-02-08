use atm0s_sdn_identity::{ConnId, NodeId};
use std::cmp::Ordering;

use super::Metric;

#[derive(Debug, Clone)]
/// conn_id, next_node, last_node, metric
pub struct Path(ConnId, NodeId, Metric);

impl Path {
    pub fn new(over: ConnId, over_node: NodeId, metric: Metric) -> Self {
        Self(over, over_node, metric)
    }

    pub fn over_node(&self) -> NodeId {
        self.1
    }

    pub fn conn(&self) -> ConnId {
        self.0
    }

    pub fn metric(&self) -> &Metric {
        &self.2
    }

    pub fn update_metric(&mut self, metric: Metric) {
        self.2 = metric;
    }
}

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use atm0s_sdn_identity::ConnId;

    use crate::core::Metric;

    use super::Path;

    #[test]
    fn test_compare_path() {
        let p1 = Path(ConnId::from_in(1, 1), 1, Metric::new(1, vec![], 10000));
        let p2 = Path(ConnId::from_in(1, 2), 2, Metric::new(1, vec![], 10000));

        assert_eq!(p1.cmp(&p2), Ordering::Equal);
        assert_eq!(p1.partial_cmp(&p2), Some(Ordering::Equal));
    }
}

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

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use bluesea_identity::ConnId;

    use crate::{Metric, Path};

    #[test]
    fn test_compare_path() {
        let p1 = Path(ConnId::from_in(1, 1), 1, Metric::new(1, vec![1], 10000));
        let p2 = Path(ConnId::from_in(1, 2), 2, Metric::new(1, vec![2], 10000));

        assert_eq!(p1.cmp(&p2), Ordering::Equal);
        assert_eq!(p1.partial_cmp(&p2), Some(Ordering::Equal));
    }
}

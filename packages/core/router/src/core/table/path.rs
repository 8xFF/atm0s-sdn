use atm0s_sdn_identity::ConnId;
use std::cmp::Ordering;

use super::Metric;

#[derive(Debug, Clone)]
/// conn_id, next_node, last_node, metric
pub struct Path(pub ConnId, pub Metric);

impl PartialOrd for Path {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
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

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use atm0s_sdn_identity::ConnId;

    use crate::core::Metric;

    use super::Path;

    #[test]
    fn test_compare_path() {
        let p1 = Path(ConnId::from_in(1, 1), Metric::new(1, vec![1], 10000));
        let p2 = Path(ConnId::from_in(1, 2), Metric::new(1, vec![2], 10000));

        assert_eq!(p1.cmp(&p2), Ordering::Equal);
        assert_eq!(p1.partial_cmp(&p2), Some(Ordering::Equal));
    }
}

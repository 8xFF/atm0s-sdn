use crate::table::metric::Metric;
use crate::table::Path;
use bluesea_identity::{ConnId, NodeId};

#[derive(Debug, Default)]
pub struct Dest {
    paths: Vec<Path>,
}

impl Dest {
    pub fn set_path(&mut self, over: ConnId, over_node: NodeId, metric: Metric) {
        match self.index_of(over) {
            Some(index) => match self.paths.get_mut(index) {
                Some(slot) => {
                    slot.2 = metric;
                }
                None => {
                    debug_assert!(false, "CANNOT_HAPPEND");
                }
            },
            None => {
                self.paths.push(Path(over, over_node, metric));
            }
        }
        self.paths.sort();
    }

    pub fn del_path(&mut self, over: ConnId) -> Option<()> {
        match self.index_of(over) {
            Some(index) => {
                self.paths.remove(index);
                Some(())
            }
            None => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// get next node to dest but not in excepts
    pub fn next(&self, excepts: &[NodeId]) -> Option<(ConnId, NodeId)> {
        for path in self.paths.iter() {
            if !excepts.contains(&path.1) {
                return Some((path.0, path.1));
            }
        }
        None
    }

    pub fn best_for(&self, neighbour_id: NodeId) -> Option<Path> {
        for path in self.paths.iter() {
            if path.1 != neighbour_id && !path.2.contain_in_hops(neighbour_id) {
                return Some(path.clone());
            }
        }
        None
    }

    pub fn next_path(&self, excepts: &[NodeId]) -> Option<Path> {
        for path in self.paths.iter() {
            if !excepts.contains(&path.1) {
                return Some(path.clone());
            }
        }
        None
    }

    fn index_of(&self, goal: ConnId) -> Option<usize> {
        for index in 0..self.paths.len() {
            match self.paths.get(index) {
                Some(path) => {
                    if path.0 == goal {
                        return Some(index);
                    }
                }
                None => {
                    debug_assert!(false, "CANNOT_HAPPEND");
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use crate::table::{Dest, Metric, Path};
    use bluesea_identity::{ConnDirection, ConnId, NodeId};

    #[test]
    fn push_sort() {
        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let conn2: ConnId = ConnId::from_out(0, 0x2);
        let node2: NodeId = 0x2;

        let conn3: ConnId = ConnId::from_out(0, 0x3);
        let node3: NodeId = 0x3;

        let mut dest = Dest::default();
        dest.set_path(conn1, node1, Metric::new(1, vec![4, 1], 1)); //directed connection
        dest.set_path(conn2, node2, Metric::new(2, vec![4, 2], 1));

        assert_eq!(dest.next(&vec![]), Some((conn1, node1)));
        assert_eq!(dest.next_path(&vec![node1]), Some(Path(conn2, node2, Metric::new(2, vec![4, 2], 1))));
        assert_eq!(dest.next_path(&vec![node2]), Some(Path(conn1, node1, Metric::new(1, vec![4, 1], 1))));
        assert_eq!(dest.next_path(&vec![node3]), Some(Path(conn1, node1, Metric::new(1, vec![4, 1], 1))));
        assert_eq!(dest.next(&vec![node1, node2]), None);
        assert_eq!(dest.next_path(&vec![node1, node2]), None);
    }

    #[test]
    fn delete_sort() {
        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let conn2: ConnId = ConnId::from_out(0, 0x2);
        let node2: NodeId = 0x2;

        let conn3: ConnId = ConnId::from_out(0, 0x3);
        let node3: NodeId = 0x3;

        let mut dest = Dest::default();
        dest.set_path(conn1, node1, Metric::new(1, vec![4, 1], 1));
        dest.set_path(conn2, node2, Metric::new(2, vec![4, 6, 2], 1));
        dest.set_path(conn3, node3, Metric::new(3, vec![4, 6, 2, 3], 1));

        dest.del_path(conn1);

        assert_eq!(dest.next(&vec![]), Some((conn2, node2)));
        assert_eq!(dest.next_path(&vec![node1]), Some(Path(conn2, node2, Metric::new(2, vec![4, 6, 2], 1))));
        assert_eq!(dest.next_path(&vec![node2]), Some(Path(conn3, node3, Metric::new(3, vec![4, 6, 2, 3], 1))));
        assert_eq!(dest.next_path(&vec![node3]), Some(Path(conn2, node2, Metric::new(2, vec![4, 6, 2], 1))));
    }

    #[test]
    fn with_hops() {
        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let conn2: ConnId = ConnId::from_out(0, 0x2);
        let node2: NodeId = 0x2;

        let conn3: ConnId = ConnId::from_out(0, 0x3);
        let node3: NodeId = 0x3;

        let conn4: ConnId = ConnId::from_out(0, 0x4);
        let node4: NodeId = 0x4;

        let mut dest = Dest::default();
        //this path from 3 => 2 => 1
        dest.set_path(conn1, node1, Metric::new(1, vec![3, 2, 1], 1));

        assert_eq!(dest.best_for(node4), Some(Path(conn1, node1, Metric::new(1, vec![3, 2, 1], 1))));
        assert_eq!(dest.best_for(node1), None);
        assert_eq!(dest.best_for(node2), None);
    }
}

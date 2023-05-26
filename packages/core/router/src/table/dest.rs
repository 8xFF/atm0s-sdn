use crate::table::metric::Metric;
use crate::table::Path;
use bluesea_identity::NodeId;

#[derive(Debug)]
pub struct Dest {
    paths: Vec<Path>,
}

impl Default for Dest {
    fn default() -> Self {
        Dest {
            paths: Default::default(),
        }
    }
}

impl Dest {
    pub fn set_path(&mut self, over: NodeId, metric: Metric) {
        match self.index_of(over) {
            Some(index) => match self.paths.get_mut(index) {
                Some(slot) => {
                    slot.1 = metric;
                }
                None => {
                    debug_assert!(false, "CANNOT_HAPPEND");
                }
            },
            None => {
                self.paths.push(Path(over, metric));
            }
        }
        self.paths.sort();
    }

    pub fn del_path(&mut self, over: NodeId) -> Option<()> {
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
    pub fn next(&self, excepts: &Vec<NodeId>) -> Option<NodeId> {
        for path in self.paths.iter() {
            if !excepts.contains(&path.0) {
                return Some(path.0);
            }
        }
        None
    }

    pub fn best_for(&self, neighbour_id: NodeId) -> Option<Path> {
        for path in self.paths.iter() {
            if path.0 != neighbour_id && !path.1.contain_in_hops(neighbour_id) {
                return Some(path.clone());
            }
        }
        None
    }

    pub fn next_path(&self, excepts: &Vec<NodeId>) -> Option<Path> {
        for path in self.paths.iter() {
            if !excepts.contains(&path.0) {
                return Some(path.clone());
            }
        }
        None
    }

    fn index_of(&self, goal: NodeId) -> Option<usize> {
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
    use bluesea_identity::NodeId;

    #[test]
    fn push_sort() {
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let node3: NodeId = 0x3;

        let mut dest = Dest::default();
        dest.set_path(node1, Metric::new(1, vec![4, 1], 1)); //directed connection
        dest.set_path(node2, Metric::new(2, vec![4, 2], 1));

        assert_eq!(dest.next(&vec![]), Some(node1));
        assert_eq!(
            dest.next_path(&vec![node1]),
            Some(Path(node2, Metric::new(2, vec![4, 2], 1)))
        );
        assert_eq!(
            dest.next_path(&vec![node2]),
            Some(Path(node1, Metric::new(1, vec![4, 1], 1)))
        );
        assert_eq!(
            dest.next_path(&vec![node3]),
            Some(Path(node1, Metric::new(1, vec![4, 1], 1)))
        );
        assert_eq!(dest.next(&vec![node1, node2]), None);
        assert_eq!(dest.next_path(&vec![node1, node2]), None);
    }

    #[test]
    fn delete_sort() {
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let node3: NodeId = 0x3;

        let mut dest = Dest::default();
        dest.set_path(node1, Metric::new(1, vec![4, 1], 1));
        dest.set_path(node2, Metric::new(2, vec![4, 6, 2], 1));
        dest.set_path(node3, Metric::new(3, vec![4, 6, 2, 3], 1));

        dest.del_path(node1);

        assert_eq!(dest.next(&vec![]), Some(node2));
        assert_eq!(
            dest.next_path(&vec![node1]),
            Some(Path(node2, Metric::new(2, vec![4, 6, 2], 1)))
        );
        assert_eq!(
            dest.next_path(&vec![node2]),
            Some(Path(node3, Metric::new(3, vec![4, 6, 2, 3], 1)))
        );
        assert_eq!(
            dest.next_path(&vec![node3]),
            Some(Path(node2, Metric::new(2, vec![4, 6, 2], 1)))
        );
    }

    #[test]
    fn with_hops() {
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let _node3: NodeId = 0x3;
        let node4: NodeId = 0x4;

        let mut dest = Dest::default();
        //this path from 3 => 2 => 1
        dest.set_path(node1, Metric::new(1, vec![3, 2, 1], 1));

        assert_eq!(
            dest.best_for(node4),
            Some(Path(node1, Metric::new(1, vec![3, 2, 1], 1)))
        );
        assert_eq!(dest.best_for(node1), None);
        assert_eq!(dest.best_for(node2), None);
    }
}

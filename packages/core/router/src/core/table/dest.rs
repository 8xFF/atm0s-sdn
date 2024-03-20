use std::collections::VecDeque;

use atm0s_sdn_identity::{ConnId, NodeId};

use super::{Metric, Path};

#[derive(Debug, PartialEq, Clone)]
pub enum DestDelta {
    SetBestPath(ConnId),
    DelBestPath,
}

#[derive(Debug, Default)]
pub struct Dest {
    paths: Vec<Path>,
    deltas: VecDeque<DestDelta>,
}

impl Dest {
    pub fn set_path(&mut self, over: ConnId, metric: Metric) {
        let pre_best_conn = self.paths.first().map(|p| p.0);
        match self.index_of(over) {
            Some(index) => {
                let slot = &mut self.paths[index];
                slot.1 = metric;
            }
            None => {
                self.paths.push(Path(over, metric));
            }
        }
        self.paths.sort();
        let after_best_conn = self.paths.first().map(|p| p.0);
        if pre_best_conn != after_best_conn {
            if let Some(conn) = after_best_conn {
                self.deltas.push_back(DestDelta::SetBestPath(conn));
            } else {
                self.deltas.push_back(DestDelta::DelBestPath);
            }
        }
    }

    pub fn del_path(&mut self, over: ConnId) -> Option<Path> {
        match self.index_of(over) {
            Some(index) => {
                if index == 0 {
                    //if remove first => changed best
                    let after_best_conn = self.paths.get(1).map(|p| p.0);
                    if let Some(conn) = after_best_conn {
                        self.deltas.push_back(DestDelta::SetBestPath(conn));
                    } else {
                        self.deltas.push_back(DestDelta::DelBestPath);
                    }
                }
                Some(self.paths.remove(index))
            }
            None => None,
        }
    }

    pub fn pop_delta(&mut self) -> Option<DestDelta> {
        self.deltas.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// get next node to dest but not in excepts
    pub fn next(&self, excepts: &[NodeId]) -> Option<(ConnId, NodeId)> {
        for path in self.paths.iter() {
            if !excepts.contains(&path.1.over_node()) {
                return Some((path.0, path.1.over_node()));
            }
        }
        None
    }

    pub fn best_for(&self, neighbour_id: NodeId) -> Option<Path> {
        for path in self.paths.iter() {
            if !path.1.contain_in_hops(neighbour_id) {
                return Some(path.clone());
            }
        }
        None
    }

    pub fn next_path(&self, excepts: &[NodeId]) -> Option<Path> {
        for path in self.paths.iter() {
            if !excepts.contains(&path.1.over_node()) {
                return Some(path.clone());
            }
        }
        None
    }

    fn index_of(&self, goal: ConnId) -> Option<usize> {
        if self.paths.is_empty() {
            return None;
        }
        for (index, path) in self.paths.iter().enumerate() {
            if path.0 == goal {
                return Some(index);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use atm0s_sdn_identity::{ConnId, NodeId};

    use crate::core::{table::Dest, DestDelta, Metric, Path};

    #[test]
    fn push_sort() {
        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let conn2: ConnId = ConnId::from_out(0, 0x2);
        let node2: NodeId = 0x2;

        let _conn3: ConnId = ConnId::from_out(0, 0x3);
        let node3: NodeId = 0x3;

        let mut dest = Dest::default();
        dest.set_path(conn1, Metric::new(1, vec![4, 1], 1)); //directed connection
        assert_eq!(dest.pop_delta(), Some(DestDelta::SetBestPath(conn1)));
        dest.set_path(conn2, Metric::new(2, vec![4, 2], 1));
        assert_eq!(dest.pop_delta(), None);

        assert_eq!(dest.next(&[]), Some((conn1, node1)));
        assert_eq!(dest.next_path(&[node1]), Some(Path(conn2, Metric::new(2, vec![4, 2], 1))));
        assert_eq!(dest.next_path(&[node2]), Some(Path(conn1, Metric::new(1, vec![4, 1], 1))));
        assert_eq!(dest.next_path(&[node3]), Some(Path(conn1, Metric::new(1, vec![4, 1], 1))));
        assert_eq!(dest.next(&[node1, node2]), None);
        assert_eq!(dest.next_path(&[node1, node2]), None);
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
        dest.set_path(conn1, Metric::new(1, vec![4, 1], 1));
        assert_eq!(dest.pop_delta(), Some(DestDelta::SetBestPath(conn1)));
        dest.set_path(conn2, Metric::new(2, vec![4, 6, 2], 1));
        dest.set_path(conn3, Metric::new(3, vec![4, 6, 2, 3], 1));
        assert_eq!(dest.pop_delta(), None);

        dest.del_path(conn1);
        assert_eq!(dest.pop_delta(), Some(DestDelta::SetBestPath(conn2)));

        assert_eq!(dest.next(&[]), Some((conn2, node2)));
        assert_eq!(dest.next_path(&[node1]), Some(Path(conn2, Metric::new(2, vec![4, 6, 2], 1))));
        assert_eq!(dest.next_path(&[node2]), Some(Path(conn3, Metric::new(3, vec![4, 6, 2, 3], 1))));
        assert_eq!(dest.next_path(&[node3]), Some(Path(conn2, Metric::new(2, vec![4, 6, 2], 1))));
    }

    #[test]
    fn with_hops() {
        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let _conn2: ConnId = ConnId::from_out(0, 0x2);
        let node2: NodeId = 0x2;

        let _conn3: ConnId = ConnId::from_out(0, 0x3);
        let _node3: NodeId = 0x3;

        let _conn4: ConnId = ConnId::from_out(0, 0x4);
        let node4: NodeId = 0x4;

        let mut dest = Dest::default();
        //this path from 3 => 2 => 1
        dest.set_path(conn1, Metric::new(1, vec![3, 2, 1], 1));

        assert_eq!(dest.best_for(node4), Some(Path(conn1, Metric::new(1, vec![3, 2, 1], 1))));
        assert_eq!(dest.best_for(node1), None);
        assert_eq!(dest.best_for(node2), None);
    }
}

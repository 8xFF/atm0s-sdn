use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::{ConnId, NodeId};
use serde::Serialize;

use super::{Metric, Path};

#[derive(Debug, PartialEq, Clone)]
pub enum RegistryDestDelta {
    SetServicePath(ConnId, NodeId, u32),
    DelServicePath(ConnId),
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct RegisterDestDump {
    next: Option<NodeId>,
    paths: HashMap<NodeId, Metric>,
}

#[derive(Debug, Default)]
pub struct RegistryDest {
    paths: Vec<Path>,
    deltas: VecDeque<RegistryDestDelta>,
}

impl RegistryDest {
    pub fn dump(&self) -> RegisterDestDump {
        RegisterDestDump {
            next: self.next(&[]).map(|p| p.1),
            paths: self.paths.iter().map(|p| (p.1.over_node(), p.1.clone())).collect(),
        }
    }

    pub fn set_path(&mut self, over: ConnId, metric: Metric) {
        match self.index_of(over) {
            Some(index) => {
                let slot = &mut self.paths[index];
                if slot.1.score() != metric.score() || slot.1.dest_node() != metric.dest_node() {
                    self.deltas.push_back(RegistryDestDelta::SetServicePath(over, metric.dest_node(), metric.score()));
                }
                slot.1 = metric;
            }
            None => {
                self.deltas.push_back(RegistryDestDelta::SetServicePath(over, metric.dest_node(), metric.score()));
                self.paths.push(Path(over, metric));
            }
        }
        self.paths.sort();
    }

    pub fn has_path(&self, over: ConnId) -> bool {
        self.index_of(over).is_some()
    }

    pub fn del_path(&mut self, over: ConnId) -> Option<Path> {
        match self.index_of(over) {
            Some(index) => {
                let path: Path = self.paths.remove(index);
                self.deltas.push_back(RegistryDestDelta::DelServicePath(over));
                Some(path)
            }
            None => None,
        }
    }

    pub fn pop_delta(&mut self) -> Option<RegistryDestDelta> {
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

    #[allow(unused)]
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
    use super::*;
    use crate::core::table::BANDWIDTH_LIMIT;

    #[test]
    fn push_sort() {
        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let conn2: ConnId = ConnId::from_out(0, 0x2);
        let node2: NodeId = 0x2;

        let _conn3: ConnId = ConnId::from_out(0, 0x3);
        let node3: NodeId = 0x3;

        let node4: NodeId = 0x4;

        let mut dest = RegistryDest::default();
        dest.set_path(conn1, Metric::new(1, vec![4, 1], BANDWIDTH_LIMIT)); //directed connection
        assert_eq!(dest.pop_delta(), Some(RegistryDestDelta::SetServicePath(conn1, node4, 21)));
        assert_eq!(dest.pop_delta(), None);
        dest.set_path(conn2, Metric::new(2, vec![4, 2], BANDWIDTH_LIMIT));
        assert_eq!(dest.pop_delta(), Some(RegistryDestDelta::SetServicePath(conn2, node4, 22)));
        assert_eq!(dest.pop_delta(), None);

        assert_eq!(dest.next(&[]), Some((conn1, node1)));
        assert_eq!(dest.next_path(&[node1]), Some(Path(conn2, Metric::new(2, vec![4, 2], BANDWIDTH_LIMIT))));
        assert_eq!(dest.next_path(&[node2]), Some(Path(conn1, Metric::new(1, vec![4, 1], BANDWIDTH_LIMIT))));
        assert_eq!(dest.next_path(&[node3]), Some(Path(conn1, Metric::new(1, vec![4, 1], BANDWIDTH_LIMIT))));
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

        let node4: NodeId = 0x4;

        let mut dest = RegistryDest::default();
        dest.set_path(conn1, Metric::new(1, vec![4, 1], BANDWIDTH_LIMIT));
        assert_eq!(dest.pop_delta(), Some(RegistryDestDelta::SetServicePath(conn1, node4, 21)));
        dest.set_path(conn2, Metric::new(2, vec![4, 6, 2], BANDWIDTH_LIMIT));
        assert_eq!(dest.pop_delta(), Some(RegistryDestDelta::SetServicePath(conn2, node4, 32)));
        dest.set_path(conn3, Metric::new(3, vec![4, 6, 2, 3], BANDWIDTH_LIMIT));
        assert_eq!(dest.pop_delta(), Some(RegistryDestDelta::SetServicePath(conn3, node4, 43)));
        assert_eq!(dest.pop_delta(), None);

        dest.del_path(conn1);
        assert_eq!(dest.pop_delta(), Some(RegistryDestDelta::DelServicePath(conn1)));

        assert_eq!(dest.next(&[]), Some((conn2, node2)));
        assert_eq!(dest.next_path(&[node1]), Some(Path(conn2, Metric::new(2, vec![4, 6, 2], BANDWIDTH_LIMIT))));
        assert_eq!(dest.next_path(&[node2]), Some(Path(conn3, Metric::new(3, vec![4, 6, 2, 3], BANDWIDTH_LIMIT))));
        assert_eq!(dest.next_path(&[node3]), Some(Path(conn2, Metric::new(2, vec![4, 6, 2], BANDWIDTH_LIMIT))));
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

        let mut dest = RegistryDest::default();
        //this path from 3 => 2 => 1
        dest.set_path(conn1, Metric::new(1, vec![3, 2, 1], BANDWIDTH_LIMIT));

        assert_eq!(dest.best_for(node4), Some(Path(conn1, Metric::new(1, vec![3, 2, 1], BANDWIDTH_LIMIT))));
        assert_eq!(dest.best_for(node1), None);
        assert_eq!(dest.best_for(node2), None);
    }
}

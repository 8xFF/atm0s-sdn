use std::collections::HashMap;

use bluesea_identity::{NodeId, NodeIdType};
use serde::{Deserialize, Serialize};
use utils::init_array::init_array;

pub use dest::Dest;
pub use metric::{Metric, BANDWIDTH_LIMIT};
pub use path::Path;

mod dest;
mod metric;
mod path;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct TableSync(pub Vec<(u8, Metric)>);

pub struct Table {
    node_id: NodeId,
    layer: u8,
    dests: [Dest; 256],
    slots: Vec<u8>,
}

impl Table {
    pub fn new(node_id: NodeId, layer: u8) -> Self {
        let node_index = node_id.layer(layer);
        let mut dests = init_array!(Dest, 256, Default::default());
        dests[node_index as usize].set_path(node_id, Metric::new(0, vec![node_id], 100000000));

        Table {
            node_id,
            layer,
            dests,
            slots: vec![node_index],
        }
    }

    pub fn slots(&self) -> Vec<u8> {
        self.slots.clone()
    }

    pub fn size(&self) -> usize {
        let mut size = 0;
        for i in 0..256 {
            if !self.dests[i].is_empty() {
                size += 1;
            }
        }
        size
    }

    pub fn add_direct(&mut self, src: NodeId, src_send_metric: Metric) {
        debug_assert_eq!(src_send_metric.hops.first(), Some(&src));
        let index = src.layer(self.layer);
        if self.dests[index as usize].is_empty() {
            log::info!(
                "[Table {}/{}] added index {} from dest {}/{:?}",
                self.node_id,
                self.layer,
                index,
                src,
                src_send_metric
            );
            self.slots.push(index);
            self.slots.sort();
        }
        self.dests[index as usize].set_path(src, src_send_metric);
    }

    pub fn del_direct(&mut self, src: NodeId) {
        for i in 0..=255 {
            let pre_empty = self.dests[i as usize].is_empty();
            self.dests[i as usize].del_path(src);
            if !pre_empty && self.dests[i as usize].is_empty() {
                log::info!(
                    "[Table {}/{}] removed index {} from dest {}",
                    self.node_id,
                    self.layer,
                    i,
                    src
                );

                if let Ok(index) = self.slots.binary_search(&i) {
                    self.slots.remove(index);
                }
            }
        }
    }

    pub fn next(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<NodeId> {
        let index = dest.layer(self.layer);
        self.dests[index as usize].next(excepts)
    }

    pub fn next_path(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<Path> {
        let index = dest.layer(self.layer);
        self.dests[index as usize].next_path(excepts)
    }

    // pub fn closest_for(&self, key: u8, excepts: &Vec<NodeId>) -> (u8, Option<NodeId>) {
    //     let current_node_index = self.node_id.layer(self.layer);
    //     if self.slots.len() <= 1 {
    //         return (current_node_index, Some(self.node_id));
    //     }
    //
    //     match self.slots.binary_search(&key) {
    //         Ok(_) => { //found => using provided slot
    //             (key, self.dests[key as usize].next(excepts))
    //         }
    //         Err(bigger_index) => {
    //             let (left, right) = {
    //                 if bigger_index < self.slots.len() { //found slot that bigger than key
    //                     if bigger_index > 0 {
    //                         (bigger_index - 1, bigger_index)
    //                     } else {
    //                         (self.slots.len() - 1, bigger_index)
    //                     }
    //                 } else {
    //                     (bigger_index - 1, 0)
    //                 }
    //             };
    //
    //             let left_value = self.slots[left];
    //             let right_value = self.slots[right];
    //             if circle_distance(left_value, key) <= circle_distance(right_value, key) {
    //                 (left_value, self.dests[left_value as usize].next(excepts))
    //             } else {
    //                 (right_value, self.dests[right_value as usize].next(excepts))
    //             }
    //         }
    //     }
    // }

    pub fn apply_sync(&mut self, src: NodeId, src_send_metric: Metric, sync: TableSync) {
        debug_assert_eq!(src_send_metric.hops.first(), Some(&src));
        log::debug!(
            "[Table {}/{}] apply sync from {} -> {}, sync {:?}",
            self.node_id,
            self.layer,
            src,
            self.node_id,
            sync.0
        );
        let mut cached: HashMap<u8, Metric> = HashMap::new();
        for (index, metric) in sync.0 {
            if let Some(sum) = metric.add(&src_send_metric) {
                cached.insert(index, sum);
            }
        }

        for i in 0..=255 as u8 {
            if i == self.node_id.layer(self.layer) {
                continue;
            }

            let dest = &mut self.dests[i as usize];
            match cached.remove(&i) {
                None => {
                    if !dest.is_empty() && src.layer(self.layer) != i {
                        // log::debug!("remove {} over {}", i, src);
                        let pre_empty = dest.is_empty();
                        dest.del_path(src);
                        if !pre_empty && dest.is_empty() {
                            log::info!(
                                "[Table {}/{}] sync => removed index {} from dest {}",
                                self.node_id,
                                self.layer,
                                i,
                                src
                            );
                            if let Ok(index) = self.slots.binary_search(&i) {
                                self.slots.remove(index);
                            }
                        }
                    }
                }
                Some(metric) => {
                    if dest.is_empty() {
                        log::info!(
                            "[Table {}/{}] sync => added index {} from dest {}/{:?}",
                            self.node_id,
                            self.layer,
                            i,
                            src,
                            src_send_metric
                        );
                        self.slots.push(i);
                        self.slots.sort();
                    }
                    dest.set_path(src, metric);
                }
            }
        }
    }

    pub fn sync_for(&self, node: NodeId) -> Option<TableSync> {
        let eq_util_layer = self.node_id.eq_util_layer(&node) as usize;
        if eq_util_layer > self.layer as usize + 1 {
            return None;
        }

        let mut res = vec![];
        for i in 0..=255 {
            let dest = &self.dests[i as usize];
            if !dest.is_empty() && i != self.node_id.layer(self.layer) {
                if let Some(Path(_over, metric)) = dest.best_for(node) {
                    res.push((i, metric));
                }
            }
        }
        Some(TableSync(res))
    }

    pub fn dump(&self) {
        let mut slots = vec![];
        let mut index = 0;
        for dest in &self.dests {
            if !dest.is_empty() {
                slots.push(index);
            }
            index += 1;
        }
        log::info!(
            "[Table {}/{}/{}] slots: {:?}",
            self.node_id,
            self.layer,
            self.node_id.layer(self.layer),
            slots
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::table::{Metric, Path, Table, TableSync};
    use bluesea_identity::{NodeId, NodeIdType};

    #[test]
    fn create_manual() {
        let node0: NodeId = 0x0;
        let mut table = Table::new(node0, 0);
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let node3: NodeId = 0x3;

        table.add_direct(node1, Metric::new(1, vec![1, 0], 1));
        table.add_direct(node2, Metric::new(2, vec![2, 4, 0], 1));
        ///fake
        table.add_direct(node3, Metric::new(1, vec![3, 0], 1));

        assert_eq!(table.slots(), vec![0, 1, 2, 3]);

        assert_eq!(table.next(node1, &vec![node2]), Some(node1));

        assert_eq!(
            table.sync_for(node1),
            Some(TableSync(vec![
                (2, Metric::new(2, vec![2, 4, 0], 1)),
                (3, Metric::new(1, vec![3, 0], 1))
            ]))
        );
        assert_eq!(
            table.sync_for(4),
            Some(TableSync(vec![
                (1, Metric::new(1, vec![1, 0], 1)),
                (3, Metric::new(1, vec![3, 0], 1))
            ]))
        );

        table.del_direct(node1);
        assert_eq!(table.next(node1, &vec![node2]), None);
    }

    #[test]
    fn create_manual_other_layer() {
        let node0: NodeId = 0x0;
        let table = Table::new(node0, 0);
        assert_eq!(table.sync_for(0x10000000), None);
    }

    #[test]
    fn apply_sync_me() {
        let node0: NodeId = 0x0;
        let mut table = Table::new(node0, 0);

        let sync = vec![(0, Metric::new(1, vec![0], 1))];
        table.apply_sync(node0, Metric::new(1, vec![0], 1), TableSync(sync));

        assert_eq!(table.slots(), vec![0]);
        assert_eq!(table.next(node0, &vec![node0]), None);
    }

    #[test]
    fn apply_sync() {
        let node0: NodeId = 0x0;
        let mut table = Table::new(node0, 0);
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let node3: NodeId = 0x3;

        table.add_direct(node1, Metric::new(1, vec![1, 0], 1));

        let sync = vec![
            (2, Metric::new(1, vec![2, 1], 1)),
            (3, Metric::new(1, vec![3, 1], 1)),
        ];
        table.apply_sync(node1, Metric::new(1, vec![1, 0], 2), TableSync(sync));

        assert_eq!(table.slots(), vec![0, 1, 2, 3]);
        assert_eq!(
            table.next_path(node1, &vec![node2]),
            Some(Path(node1, Metric::new(1, vec![1, 0], 1)))
        );
        assert_eq!(
            table.next_path(node2, &vec![node2]),
            Some(Path(node1, Metric::new(2, vec![2, 1, 0], 1)))
        );
        assert_eq!(
            table.next_path(node3, &vec![node2]),
            Some(Path(node1, Metric::new(2, vec![3, 1, 0], 1)))
        );

        let sync = vec![(3, Metric::new(1, vec![3, 1], 1))];
        table.apply_sync(node1, Metric::new(1, vec![1, 0], 1), TableSync(sync));
        assert_eq!(table.next(node1, &vec![node2]), Some(node1));
        assert_eq!(table.next(node2, &vec![node2]), None);
        assert_eq!(table.next(node3, &vec![node2]), Some(node1));
    }

    #[test]
    fn apply_sync_multi() {
        /**
        A --- B -2- D
        |     |    |
        |     2    |
        |     |    |
        | --- C --
        **/
        let nodeA: NodeId = 0x0;
        let nodeB: NodeId = 0x1;
        let nodeC: NodeId = 0x2;
        let nodeD: NodeId = 0x3;

        let mut tableA = Table::new(nodeA, 0);

        tableA.add_direct(nodeB, Metric::new(1, vec![nodeB, nodeA], 1));
        tableA.add_direct(nodeC, Metric::new(1, vec![nodeC, nodeA], 1));

        let sync1 = vec![
            (nodeC.layer(0), Metric::new(1, vec![nodeC, nodeB], 1)),
            (nodeD.layer(0), Metric::new(2, vec![nodeD, nodeB], 1)),
        ];
        tableA.apply_sync(
            nodeB,
            Metric::new(1, vec![nodeB, nodeA], 1),
            TableSync(sync1),
        );

        let sync2 = vec![
            (nodeB.layer(0), Metric::new(2, vec![nodeB, nodeC], 2)),
            (nodeD.layer(0), Metric::new(1, vec![nodeD, nodeC], 1)),
        ];
        tableA.apply_sync(
            nodeC,
            Metric::new(1, vec![nodeC, nodeA], 1),
            TableSync(sync2),
        );

        assert_eq!(
            tableA.next_path(nodeB, &vec![]),
            Some(Path(nodeB, Metric::new(1, vec![nodeB, nodeA], 1)))
        );
        assert_eq!(tableA.next(nodeC, &vec![]), Some(nodeC));
        assert_eq!(tableA.next(nodeD, &vec![]), Some(nodeC));

        /**
        A --- B -2- D
        |     |
        |     2
        |     |
        | --- C
        **/
        let sync2 = vec![(nodeB.layer(0), Metric::new(2, vec![nodeB, nodeC], 1))];
        tableA.apply_sync(
            nodeC,
            Metric::new(1, vec![nodeC, nodeA], 1),
            TableSync(sync2),
        );

        assert_eq!(tableA.next(nodeB, &vec![]), Some(nodeB));
        assert_eq!(tableA.next(nodeC, &vec![]), Some(nodeC));
        assert_eq!(tableA.next(nodeD, &vec![]), Some(nodeB));
    }

    // #[test]
    // fn closest_key() {
    //     let node0: NodeId = 0x0;
    //     let mut table = Table::new(node0, 0);
    //
    //     assert_eq!(table.closest_for(0, &vec![]), (0, Some(0)));
    //     assert_eq!(table.closest_for(100, &vec![]), (0, Some(0)));
    //
    //     table.add_direct(1, Metric::new(1, vec![1, 0], 1));
    //     table.add_direct(5, Metric::new(1, vec![5, 0], 1));
    //     table.add_direct(40, Metric::new(1, vec![40, 0], 1));
    //
    //     assert_eq!(table.closest_for(0, &vec![]), (0, Some(0)));
    //     assert_eq!(table.closest_for(3, &vec![]), (1, Some(1)));
    //     assert_eq!(table.closest_for(3, &vec![1]), (1, None));
    //
    //     assert_eq!(table.closest_for(4, &vec![]), (5, Some(5)));
    //     assert_eq!(table.closest_for(20, &vec![]), (5, Some(5)));
    //     assert_eq!(table.closest_for(40, &vec![]), (40, Some(40)));
    //     assert_eq!(table.closest_for(41, &vec![]), (40, Some(40)));
    //
    //     assert_eq!(table.closest_for(254, &vec![]), (0, Some(0)));
    // }
    //
    // #[test]
    // fn closest_key_strong_order() {
    //     let mut table_1 = Table::new(0x00, 0);
    //     let mut table_2 = Table::new(0x01, 0);
    //
    //     table_1.add_direct(0x01, Metric::new(1, vec![1, 0], 1));
    //     table_2.add_direct(0x00, Metric::new(1, vec![0, 1], 1));
    //
    //     assert_eq!(table_1.closest_for(0x82, &vec![]), table_2.closest_for(0x82, &vec![]));
    // }
}

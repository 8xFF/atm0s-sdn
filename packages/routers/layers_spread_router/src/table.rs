use std::collections::HashMap;

use atm0s_sdn_identity::{ConnId, NodeId, NodeIdType};
use atm0s_sdn_utils::init_array::init_array;
use serde::{Deserialize, Serialize};

pub use dest::Dest;
pub use metric::{Metric, BANDWIDTH_LIMIT};
pub use path::Path;

mod dest;
mod metric;
mod path;

/// Index of node-id inside this table (0-255)
pub type NodeIndex = u8;

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
        let dests = init_array!(Dest, 256, Default::default());
        Table { node_id, layer, dests, slots: vec![] }
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

    pub fn add_direct(&mut self, src_conn: ConnId, src: NodeId, src_send_metric: Metric) {
        debug_assert_eq!(src_send_metric.hops.first(), Some(&src));
        let index = src.layer(self.layer);
        if self.dests[index as usize].is_empty() {
            log::info!("[Table {}/{}] added index {} from dest {}/{}/{:?}", self.node_id, self.layer, index, src_conn, src, src_send_metric);
            self.slots.push(index);
            self.slots.sort();
        }
        self.dests[index as usize].set_path(src_conn, src, src_send_metric);
    }

    pub fn del_direct(&mut self, src_conn: ConnId) {
        for i in 0..=255 {
            let pre_empty = self.dests[i as usize].is_empty();
            self.dests[i as usize].del_path(src_conn);
            if !pre_empty && self.dests[i as usize].is_empty() {
                log::info!("[Table {}/{}] removed index {} from dest {}", self.node_id, self.layer, i, src_conn);

                if let Ok(index) = self.slots.binary_search(&i) {
                    self.slots.remove(index);
                }
            }
        }
    }

    pub fn next(&self, dest: NodeId, excepts: &[NodeId]) -> Option<(ConnId, NodeId)> {
        let index = dest.layer(self.layer);
        self.dests[index as usize].next(excepts)
    }

    pub fn next_path(&self, dest: NodeId, excepts: &[NodeId]) -> Option<Path> {
        let index = dest.layer(self.layer);
        self.dests[index as usize].next_path(excepts)
    }

    pub fn closest_for(&self, key: u8, excepts: &[NodeId]) -> Option<(NodeIndex, ConnId, NodeId)> {
        let mut closest_distance: u16 = 256;
        let mut res = None;
        for slot in &self.slots {
            let distance = (*slot ^ key) as u16;
            if distance < closest_distance {
                if let Some((conn, node)) = self.dests[*slot as usize].next(excepts) {
                    res = Some((*slot, conn, node));
                    closest_distance = distance;
                }
            }
        }

        res
    }

    pub fn apply_sync(&mut self, src_conn: ConnId, src: NodeId, src_send_metric: Metric, sync: TableSync) {
        debug_assert_eq!(src_send_metric.hops.first(), Some(&src));
        log::debug!("[Table {}/{}] apply sync from {}/{} -> {}, sync {:?}", self.node_id, self.layer, src_conn, src, self.node_id, sync.0);
        let mut cached: HashMap<u8, Metric> = HashMap::new();
        for (index, metric) in sync.0 {
            if let Some(sum) = metric.add(&src_send_metric) {
                cached.insert(index, sum);
            }
        }

        for i in 0..=255_u8 {
            if i == self.node_id.layer(self.layer) {
                continue;
            }

            let dest = &mut self.dests[i as usize];
            match cached.remove(&i) {
                None => {
                    if !dest.is_empty() && src.layer(self.layer) != i {
                        // log::debug!("remove {} over {}", i, src);
                        let pre_empty = dest.is_empty();
                        dest.del_path(src_conn);
                        if !pre_empty && dest.is_empty() {
                            log::info!("[Table {}/{}] sync => removed index {} from dest {}", self.node_id, self.layer, i, src);
                            if let Ok(index) = self.slots.binary_search(&i) {
                                self.slots.remove(index);
                            }
                        }
                    }
                }
                Some(metric) => {
                    if dest.is_empty() {
                        log::info!("[Table {}/{}] sync => added index {} from dest {}/{}/{:?}", self.node_id, self.layer, i, src_conn, src, src_send_metric);
                        self.slots.push(i);
                        self.slots.sort();
                    }
                    dest.set_path(src_conn, src, metric);
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
                if let Some(Path(_over, _over_node, metric)) = dest.best_for(node) {
                    res.push((i, metric));
                }
            }
        }
        Some(TableSync(res))
    }

    pub fn log_dump(&self) {
        let mut slots = vec![];
        for (index, dest) in self.dests.iter().enumerate() {
            if !dest.is_empty() {
                slots.push(index);
            }
        }
        log::info!("[Table {}/{}/{}] slots: {:?}", self.node_id, self.layer, self.node_id.layer(self.layer), slots);
    }

    pub fn print_dump(&self) {
        let mut slots = vec![];
        for (index, dest) in self.dests.iter().enumerate() {
            if !dest.is_empty() {
                slots.push(index);
            }
        }
        println!("[Table {}/{}/{}] slots: {:?}", self.node_id, self.layer, self.node_id.layer(self.layer), slots);
    }
}

#[cfg(test)]
mod tests {
    use crate::table::{Metric, Path, Table, TableSync};
    use atm0s_sdn_identity::{ConnId, NodeId, NodeIdType};

    #[test]
    fn create_manual() {
        let node0: NodeId = 0x0;
        let mut table = Table::new(node0, 0);
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let node3: NodeId = 0x3;

        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let conn2: ConnId = ConnId::from_out(0, 0x2);
        let conn3: ConnId = ConnId::from_out(0, 0x3);

        table.add_direct(conn1, node1, Metric::new(1, vec![1, 0], 1));
        table.add_direct(conn2, node2, Metric::new(2, vec![2, 4, 0], 1));
        // fake
        table.add_direct(conn3, node3, Metric::new(1, vec![3, 0], 1));

        assert_eq!(table.slots(), vec![1, 2, 3]);

        assert_eq!(table.next(node1, &[node2]), Some((conn1, node1)));

        assert_eq!(table.sync_for(node1), Some(TableSync(vec![(2, Metric::new(2, vec![2, 4, 0], 1)), (3, Metric::new(1, vec![3, 0], 1))])));
        assert_eq!(table.sync_for(4), Some(TableSync(vec![(1, Metric::new(1, vec![1, 0], 1)), (3, Metric::new(1, vec![3, 0], 1))])));

        table.del_direct(conn1);
        assert_eq!(table.next(node1, &[node2]), None);
    }

    #[test]
    fn create_manual_other_layer() {
        let node0: NodeId = 0x0;
        let table = Table::new(node0, 0);
        assert_eq!(table.sync_for(0x10000000), None);
    }

    // we dont sync with self, because it don't has connection_id
    // #[test]
    // fn apply_sync_me() {
    //     let node0: NodeId = 0x0;
    //     let mut table = Table::new(node0, 0);
    //
    //     let sync = vec![(0, Metric::new(1, vec![0], 1))];
    //     table.apply_sync(node0, Metric::new(1, vec![0], 1), TableSync(sync));
    //
    //     assert_eq!(table.slots(), vec![0]);
    //     assert_eq!(table.next(node0, &[node0]), None);
    // }

    #[test]
    fn apply_sync() {
        let node0: NodeId = 0x0;
        let mut table = Table::new(node0, 0);
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let node3: NodeId = 0x3;

        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let _conn2: ConnId = ConnId::from_out(0, 0x2);
        let _conn3: ConnId = ConnId::from_out(0, 0x3);

        table.add_direct(conn1, node1, Metric::new(1, vec![1, 0], 1));

        let sync = vec![(2, Metric::new(1, vec![2, 1], 1)), (3, Metric::new(1, vec![3, 1], 1))];
        table.apply_sync(conn1, node1, Metric::new(1, vec![1, 0], 2), TableSync(sync));

        assert_eq!(table.slots(), vec![1, 2, 3]);
        assert_eq!(table.next_path(node1, &[node2]), Some(Path(conn1, node1, Metric::new(1, vec![1, 0], 1))));
        assert_eq!(table.next_path(node2, &[node2]), Some(Path(conn1, node1, Metric::new(2, vec![2, 1, 0], 1))));
        assert_eq!(table.next_path(node3, &[node2]), Some(Path(conn1, node1, Metric::new(2, vec![3, 1, 0], 1))));

        let sync = vec![(3, Metric::new(1, vec![3, 1], 1))];
        table.apply_sync(conn1, node1, Metric::new(1, vec![1, 0], 1), TableSync(sync));
        assert_eq!(table.next(node1, &[node2]), Some((conn1, node1)));
        assert_eq!(table.next(node2, &[node2]), None);
        assert_eq!(table.next(node3, &[node2]), Some((conn1, node1)));
    }

    #[test]
    fn apply_sync_multi() {
        // A --- B -2- D
        // |     |    |
        // |     2    |
        // |     |    |
        // | --- C ---

        let node_a: NodeId = 0x0;
        let node_b: NodeId = 0x1;
        let node_c: NodeId = 0x2;
        let node_d: NodeId = 0x3;

        let conn_b: ConnId = ConnId::from_out(0, 0x1);
        let conn_c: ConnId = ConnId::from_out(0, 0x2);
        let _conn_d: ConnId = ConnId::from_out(0, 0x3);

        let mut table_a = Table::new(node_a, 0);

        table_a.add_direct(conn_b, node_b, Metric::new(1, vec![node_b, node_a], 1));
        table_a.add_direct(conn_c, node_c, Metric::new(1, vec![node_c, node_a], 1));

        let sync1 = vec![(node_c.layer(0), Metric::new(1, vec![node_c, node_b], 1)), (node_d.layer(0), Metric::new(2, vec![node_d, node_b], 1))];
        table_a.apply_sync(conn_b, node_b, Metric::new(1, vec![node_b, node_a], 1), TableSync(sync1));

        let sync2 = vec![(node_b.layer(0), Metric::new(2, vec![node_b, node_c], 2)), (node_d.layer(0), Metric::new(1, vec![node_d, node_c], 1))];
        table_a.apply_sync(conn_c, node_c, Metric::new(1, vec![node_c, node_a], 1), TableSync(sync2));

        assert_eq!(table_a.next_path(node_b, &[]), Some(Path(conn_b, node_b, Metric::new(1, vec![node_b, node_a], 1))));
        assert_eq!(table_a.next(node_c, &[]), Some((conn_c, node_c)));
        assert_eq!(table_a.next(node_d, &[]), Some((conn_c, node_c)));

        // A --- B -2- D
        // |     |
        // |     2
        // |     |
        // | --- C

        let sync2 = vec![(node_b.layer(0), Metric::new(2, vec![node_b, node_c], 1))];
        table_a.apply_sync(conn_c, node_c, Metric::new(1, vec![node_c, node_a], 1), TableSync(sync2));

        assert_eq!(table_a.next(node_b, &[]), Some((conn_b, node_b)));
        assert_eq!(table_a.next(node_c, &[]), Some((conn_c, node_c)));
        assert_eq!(table_a.next(node_d, &[]), Some((conn_b, node_b)));
    }

    #[test]
    fn closest_key() {
        let node0: NodeId = 0x0;
        let mut table = Table::new(node0, 0);

        assert_eq!(table.closest_for(0, &[]), None);
        assert_eq!(table.closest_for(100, &[]), None);

        let conn1 = ConnId::from_out(0, 1);
        let conn5 = ConnId::from_out(0, 5);
        let conn40 = ConnId::from_out(0, 40);

        table.add_direct(conn1, 1, Metric::new(1, vec![1, 0], 1));
        table.add_direct(conn5, 5, Metric::new(1, vec![5, 0], 1));
        table.add_direct(conn40, 40, Metric::new(1, vec![40, 0], 1));

        assert_eq!(table.closest_for(0, &[]), Some((1, conn1, 1)));
        assert_eq!(table.closest_for(3, &[]), Some((1, conn1, 1)));
        assert_eq!(table.closest_for(3, &[1]), Some((5, conn5, 5)));

        assert_eq!(table.closest_for(4, &[]), Some((5, conn5, 5)));
        assert_eq!(table.closest_for(20, &[]), Some((5, conn5, 5)));
        assert_eq!(table.closest_for(40, &[]), Some((40, conn40, 40)));
        assert_eq!(table.closest_for(41, &[]), Some((40, conn40, 40)));

        assert_eq!(table.closest_for(254, &[]), Some((40, conn40, 40)));
    }
}

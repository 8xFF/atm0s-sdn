use atm0s_sdn_identity::{ConnId, NodeId, NodeIdType};
use serde::{Deserialize, Serialize};

use crate::core::{Metric, Path};
use crate::core::{Registry, RegistrySync};

use super::registry::RegistryDelta;
use super::table::{NodeIndex, Table, TableDelta, TableSync};
use super::ServiceDestination;

#[derive(Debug, PartialEq, Clone)]
pub enum RouterDelta {
    Table(u8, TableDelta),
    Registry(RegistryDelta),
}

/// Which layer in node id space, in this case is 0 -> 3
pub type Layer = u8;

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct RouterSync(pub RegistrySync, pub [Option<TableSync>; 4]);

pub struct Router {
    node_id: NodeId,
    tables: [Table; 4],
    service_registry: Registry,
}

impl Router {
    pub fn new(local_node_id: NodeId) -> Self {
        let tables = [Table::new(local_node_id, 0), Table::new(local_node_id, 1), Table::new(local_node_id, 2), Table::new(local_node_id, 3)];

        Router {
            node_id: local_node_id,
            tables,
            service_registry: Registry::new(local_node_id),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    pub fn size(&self) -> usize {
        let mut size = 0;
        for i in 0..4 {
            size += self.tables[i].size();
        }
        size
    }

    pub fn register_service(&mut self, service_id: u8) {
        self.service_registry.add_service(service_id);
    }

    pub fn service_next(&self, service_id: u8, excepts: &[NodeId]) -> Option<ServiceDestination> {
        self.service_registry.next(service_id, excepts)
    }

    pub fn set_direct(&mut self, over: ConnId, metric: Metric) {
        let over_node = metric.over_node();
        let eq_util_layer = self.node_id.eq_util_layer(&over_node) as usize;
        log::debug!("[Router {}] set_direct {}/{} with metric {:?}, eq_util_layer {}", self.node_id, over, over_node, metric, eq_util_layer);
        assert!(eq_util_layer <= 4);
        debug_assert!(eq_util_layer > 0, "wrong eq_layer {} {}", self.node_id, over_node);
        if eq_util_layer > 0 {
            self.tables[eq_util_layer - 1].add_direct(over, metric.clone());
        }
    }

    pub fn del_direct(&mut self, over: ConnId) {
        log::debug!("[Router {}] del_direct {}", self.node_id, over);
        for table in &mut self.tables {
            table.del_direct(over);
        }
        self.service_registry.del_direct(over);
    }

    pub fn next(&self, dest: NodeId, excepts: &[NodeId]) -> Option<(ConnId, NodeId)> {
        let eq_util_layer = self.node_id.eq_util_layer(&dest) as usize;
        debug_assert!(eq_util_layer <= 4);
        if eq_util_layer == 0 {
            None
        } else {
            self.tables.get(eq_util_layer - 1)?.next(dest, excepts)
        }
    }

    pub fn next_path(&self, dest: NodeId, excepts: &[NodeId]) -> Option<Path> {
        let eq_util_layer = self.node_id.eq_util_layer(&dest) as usize;
        debug_assert!(eq_util_layer <= 4);
        if eq_util_layer == 0 {
            None
        } else {
            self.tables.get(eq_util_layer - 1)?.next_path(dest, excepts)
        }
    }

    pub fn closest_node(&self, key: NodeId, excepts: &[NodeId]) -> Option<(ConnId, NodeId, Layer, NodeIndex)> {
        for i in [3, 2, 1, 0] {
            let index = key.layer(i);
            if let Some((next_index, next_conn, next_node)) = self.tables[i as usize].closest_for(index, excepts) {
                let next_distance = next_index ^ index;
                let current_index = self.node_id.layer(i);
                let current_distance = index ^ current_index;
                if current_distance > next_distance {
                    return Some((next_conn, next_node, i, next_index));
                }
            } else {
                //if find nothing => that mean this layer is empty trying to find closest node in next layer
                continue;
            };
        }

        None
    }

    pub fn create_sync(&self, for_node: NodeId) -> RouterSync {
        RouterSync(
            self.service_registry.sync_for(for_node),
            [
                self.tables[0].sync_for(for_node),
                self.tables[1].sync_for(for_node),
                self.tables[2].sync_for(for_node),
                self.tables[3].sync_for(for_node),
            ],
        )
    }

    pub fn apply_sync(&mut self, conn: ConnId, metric: Metric, sync: RouterSync) {
        self.service_registry.apply_sync(conn, metric.clone(), sync.0);
        for (index, table_sync) in sync.1.into_iter().enumerate() {
            if let Some(table_sync) = table_sync {
                self.tables[index].apply_sync(conn, metric.clone(), table_sync);
            }
        }
    }

    pub fn pop_delta(&mut self) -> Option<RouterDelta> {
        if let Some(delta) = self.service_registry.pop_delta() {
            return Some(RouterDelta::Registry(delta));
        }
        for (layer, table) in &mut self.tables.iter_mut().enumerate() {
            if let Some(delta) = table.pop_delta() {
                return Some(RouterDelta::Table(layer as u8, delta));
            }
        }
        None
    }

    pub fn log_dump(&self) {
        self.service_registry.log_dump();
        log::debug!("[Router {}] dump begin", self.node_id);
        for i in 0..4 {
            self.tables[3 - i].log_dump();
        }
        log::debug!("[Router {}] dump end", self.node_id);
    }

    pub fn print_dump(&self) {
        self.service_registry.print_dump();
        println!("[Router {}] dump begin", self.node_id);
        for i in 0..4 {
            self.tables[3 - i].print_dump();
        }
        println!("[Router {}] dump end", self.node_id);
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use atm0s_sdn_identity::{ConnId, NodeId, NodeIdType};

    use crate::core::registry::REGISTRY_LOCAL_BW;
    use crate::core::{table::TableSync, Metric, Path, Router, RouterSync};
    use crate::core::{RegistrySync, ServiceDestination};

    #[test]
    fn create_manual_multi_layers() {
        let node0: NodeId = 0x0;
        let node1: NodeId = 0x1;
        let node1_conn: ConnId = ConnId::from_out(0, 0x1);
        let node2: NodeId = 0x2;
        let _node3: NodeId = 0x3;
        let z_node1: NodeId = 0x01000001;
        let z_node1_conn: ConnId = ConnId::from_out(0, 0x01000001);
        let z_node2: NodeId = 0x01000002;

        let mut router = Router::new(node0);

        assert_eq!(router.node_id(), node0);
        assert_eq!(router.size(), 0);

        router.set_direct(z_node1_conn, Metric::new(1, vec![z_node1], 1));
        router.set_direct(node1_conn, Metric::new(1, vec![node1], 1));

        assert_eq!(router.size(), 2);

        //same zone, group
        assert_eq!(router.next(node1, &[]), Some((node1_conn, node1)));
        assert_eq!(router.next_path(node1, &[]), Some(Path(node1_conn, Metric::new(1, vec![node1], 1))));
        assert_eq!(router.next(node2, &[]), None);
        assert_eq!(router.next_path(node2, &[]), None);

        //other zone
        assert_eq!(router.next(z_node2, &[]), Some((z_node1_conn, z_node1)));
    }

    fn create_router(node_id: NodeId) -> (NodeId, ConnId, Router) {
        (node_id, ConnId::from_out(0, node_id as u64), Router::new(node_id))
    }

    #[test]
    fn simple_relay_route() {
        // 2 - 1 - 3
        let (_node2, _conn2, mut router2) = create_router(2);
        router2.set_direct(ConnId::from_in(0, 0), Metric::new(0, vec![1], 0));
        assert_eq!(router2.tables[0].slots(), vec![1]);

        router2.apply_sync(
            ConnId::from_in(0, 0),
            Metric::new(0, vec![1], 0),
            RouterSync(RegistrySync(vec![]), [Some(TableSync(vec![(3, Metric::new(0, vec![3], 0))])), None, None, None]),
        );
        assert_eq!(router2.tables[0].slots(), vec![1, 3]);
    }

    #[test]
    fn complex_sync_same_zone() {
        // A -1- B -1- C -1- F
        // |1    |1
        // D -2- E
        let (node_a, conn_a, mut router_a) = create_router(0x01);
        let (node_b, conn_b, mut router_b) = create_router(0x02);
        let (node_c, conn_c, mut router_c) = create_router(0x03);
        let (node_d, conn_d, mut router_d) = create_router(0x04);
        let (node_e, conn_e, mut router_e) = create_router(0x05);
        let (node_f, conn_f, mut router_f) = create_router(0x06);

        let metric_registry_local = Metric::new(0, vec![node_a], REGISTRY_LOCAL_BW);

        router_a.register_service(1);
        router_a.set_direct(conn_d, Metric::new(1, vec![node_d], 1));
        router_a.set_direct(conn_b, Metric::new(1, vec![node_b], 1));

        router_b.set_direct(conn_a, Metric::new(1, vec![node_a], 1));
        router_b.set_direct(conn_c, Metric::new(1, vec![node_c], 1));
        router_b.set_direct(conn_e, Metric::new(1, vec![node_e], 1));

        router_c.set_direct(conn_b, Metric::new(1, vec![node_b], 1));
        router_c.set_direct(conn_f, Metric::new(1, vec![node_f], 1));

        router_d.set_direct(conn_a, Metric::new(1, vec![node_a], 1));
        router_d.set_direct(conn_e, Metric::new(2, vec![node_e], 2));

        router_e.set_direct(conn_d, Metric::new(2, vec![node_d], 2));
        router_e.set_direct(conn_b, Metric::new(1, vec![node_b], 1));

        router_f.set_direct(conn_c, Metric::new(1, vec![node_c], 1));

        let empty_sync = TableSync(Vec::new());

        //create sync
        let sync_a_b = router_a.create_sync(node_b);
        assert_eq!(
            sync_a_b,
            RouterSync(
                RegistrySync(vec![(1, metric_registry_local.clone())]),
                [
                    Some(TableSync(vec![(4, Metric::new(1, vec![node_d], 1))])),
                    Some(empty_sync.clone()),
                    Some(empty_sync.clone()),
                    Some(empty_sync.clone())
                ]
            )
        );

        for _i in 0..4 {
            //for node A
            let sync_a_b = router_a.create_sync(node_b);
            let sync_a_d = router_a.create_sync(node_d);
            router_b.apply_sync(conn_a, Metric::new(1, vec![node_a], 1), sync_a_b);
            router_d.apply_sync(conn_a, Metric::new(1, vec![node_a], 1), sync_a_d);

            //for node B
            let sync_b_a = router_b.create_sync(node_a);
            let sync_b_c = router_b.create_sync(node_c);
            let sync_b_e = router_b.create_sync(node_e);
            router_a.apply_sync(conn_b, Metric::new(1, vec![node_b], 1), sync_b_a);
            router_c.apply_sync(conn_b, Metric::new(1, vec![node_b], 1), sync_b_c);
            router_e.apply_sync(conn_b, Metric::new(1, vec![node_b], 1), sync_b_e);

            //for node C
            let sync_c_b = router_c.create_sync(node_b);
            let sync_c_f = router_c.create_sync(node_f);
            router_b.apply_sync(conn_c, Metric::new(1, vec![node_c], 1), sync_c_b);
            router_f.apply_sync(conn_c, Metric::new(1, vec![node_c], 1), sync_c_f);

            //for node D
            let sync_d_a = router_d.create_sync(node_a);
            let sync_d_e = router_d.create_sync(node_e);
            router_a.apply_sync(conn_d, Metric::new(1, vec![node_d], 1), sync_d_a);
            router_e.apply_sync(conn_d, Metric::new(2, vec![node_d], 2), sync_d_e);

            //for node E
            let sync_e_b = router_e.create_sync(node_b);
            let sync_e_d = router_e.create_sync(node_d);
            router_b.apply_sync(conn_e, Metric::new(1, vec![node_e], 1), sync_e_b);
            router_d.apply_sync(conn_e, Metric::new(2, vec![node_e], 2), sync_e_d);

            //for node F
            let sync_f_c = router_f.create_sync(node_c);
            router_c.apply_sync(conn_f, Metric::new(1, vec![node_f], 1), sync_f_c);
        }

        //A -1- B -1- C -1- F
        //|1    |1
        //D -2- E

        assert_eq!(router_a.next_path(node_b, &[]), Some(Path(conn_b, Metric::new(1, vec![node_b], 1))));
        assert_eq!(router_a.next_path(node_c, &[]), Some(Path(conn_b, Metric::new(2, vec![node_c, node_b], 1))));
        assert_eq!(router_a.next_path(node_d, &[]), Some(Path(conn_d, Metric::new(1, vec![node_d], 1))));
        assert_eq!(router_a.next_path(node_e, &[]), Some(Path(conn_b, Metric::new(2, vec![node_e, node_b], 1))));
        assert_eq!(router_a.next_path(node_f, &[]), Some(Path(conn_b, Metric::new(3, vec![node_f, node_c, node_b], 1))));
        assert_eq!(router_f.service_next(1, &[]), Some(ServiceDestination::Remote(conn_c, node_c)));
        assert_eq!(router_c.service_next(1, &[]), Some(ServiceDestination::Remote(conn_b, node_b)));
        assert_eq!(router_b.service_next(1, &[]), Some(ServiceDestination::Remote(conn_a, node_a)));
        assert_eq!(router_e.service_next(1, &[]), Some(ServiceDestination::Remote(conn_b, node_b)));
        assert_eq!(router_d.service_next(1, &[]), Some(ServiceDestination::Remote(conn_a, node_a)));

        //remove A - B
        //
        //A     B -1- C -1- F
        //|1    |1
        //D -2- E

        router_a.del_direct(conn_b);
        router_b.del_direct(conn_a);

        for _i in 0..4 {
            //for node A
            let sync_a_d = router_a.create_sync(node_d);
            router_d.apply_sync(conn_a, Metric::new(1, vec![node_a], 1), sync_a_d);

            //for node B
            let sync_b_c = router_b.create_sync(node_c);
            let sync_b_e = router_b.create_sync(node_e);
            router_c.apply_sync(conn_b, Metric::new(1, vec![node_b], 1), sync_b_c);
            router_e.apply_sync(conn_b, Metric::new(1, vec![node_b], 1), sync_b_e);

            //for node C
            let sync_c_b = router_c.create_sync(node_b);
            let sync_c_f = router_c.create_sync(node_f);
            router_b.apply_sync(conn_c, Metric::new(1, vec![node_c], 1), sync_c_b);
            router_f.apply_sync(conn_c, Metric::new(1, vec![node_c], 1), sync_c_f);

            //for node D
            let sync_d_a = router_d.create_sync(node_a);
            let sync_d_e = router_d.create_sync(node_e);
            router_a.apply_sync(conn_d, Metric::new(1, vec![node_d], 1), sync_d_a);
            router_e.apply_sync(conn_d, Metric::new(2, vec![node_d], 2), sync_d_e);

            //for node E
            let sync_e_b = router_e.create_sync(node_b);
            let sync_e_d = router_e.create_sync(node_d);
            router_b.apply_sync(conn_e, Metric::new(1, vec![node_e], 1), sync_e_b);
            router_d.apply_sync(conn_e, Metric::new(2, vec![node_e], 2), sync_e_d);

            //for node F
            let sync_f_c = router_f.create_sync(node_c);
            router_c.apply_sync(conn_f, Metric::new(1, vec![node_f], 1), sync_f_c);
        }

        // remove A - B
        // A     B -1- C -1- F
        // |1    |1
        // D -2- E

        assert_eq!(router_a.next_path(node_b, &[node_a]), Some(Path(conn_d, Metric::new(4, vec![node_b, node_e, node_d], 1))));
        assert_eq!(router_a.next_path(node_c, &[node_a]), Some(Path(conn_d, Metric::new(5, vec![node_c, node_b, node_e, node_d], 1))));
        assert_eq!(router_a.next_path(node_d, &[node_a]), Some(Path(conn_d, Metric::new(1, vec![node_d], 1))));
        assert_eq!(router_a.next_path(node_e, &[node_a]), Some(Path(conn_d, Metric::new(3, vec![node_e, node_d], 1))));
        assert_eq!(
            router_a.next_path(node_f, &[node_a]),
            Some(Path(conn_d, Metric::new(6, vec![node_f, node_c, node_b, node_e, node_d], 1)))
        );
    }

    #[test]
    fn closest_node() {
        let (_node_a, _conn_a, mut router_a) = create_router(NodeId::build(0, 0, 0, 1));

        assert_eq!(router_a.closest_node(0x01, &[]), None);

        let node_5000 = NodeId::build(5, 0, 0, 1);
        let node_0500 = NodeId::build(0, 5, 0, 1);

        let conn_5000 = ConnId::from_out(0, 5000);
        let conn_0500 = ConnId::from_out(0, 500);

        router_a.set_direct(conn_5000, Metric::new(1, vec![node_5000], 1));
        router_a.set_direct(conn_0500, Metric::new(1, vec![node_0500], 1));

        assert_eq!(router_a.closest_node(NodeId::build(1, 0, 0, 0), &[]), None);
        assert_eq!(router_a.closest_node(NodeId::build(4, 0, 0, 0), &[]), Some((conn_5000, node_5000, 3, 5)));
        assert_eq!(router_a.closest_node(NodeId::build(6, 0, 0, 0), &[]), Some((conn_5000, node_5000, 3, 5)));

        assert_eq!(router_a.closest_node(NodeId::build(0, 1, 0, 0), &[]), None);
        assert_eq!(router_a.closest_node(NodeId::build(0, 6, 0, 0), &[]), Some((conn_0500, node_0500, 2, 5)));
        assert_eq!(router_a.closest_node(NodeId::build(2, 1, 0, 0), &[]), None);
        assert_eq!(router_a.closest_node(NodeId::build(2, 6, 0, 0), &[]), Some((conn_0500, node_0500, 2, 5)));
    }

    /// This test ensure closest_node working when we have only small part of key-space
    #[test]
    fn closest_node_out_of_space() {
        let (_node_a, _conn_a, mut router_a) = create_router(NodeId::build(1, 0, 0, 1));

        assert_eq!(router_a.closest_node(0x01, &[]), None);

        let node_0002 = NodeId::build(1, 0, 0, 2);
        let node_0003 = NodeId::build(1, 0, 0, 3);

        let conn_0002 = ConnId::from_out(0, 2);
        let conn_0003 = ConnId::from_out(0, 3);

        router_a.set_direct(conn_0002, Metric::new(1, vec![node_0002], 1));
        router_a.set_direct(conn_0003, Metric::new(1, vec![node_0003], 1));

        assert_eq!(router_a.closest_node(NodeId::build(1, 0, 0, 0), &[]), None);
        assert_eq!(router_a.closest_node(NodeId::build(4, 0, 0, 0), &[]), None);
        assert_eq!(router_a.closest_node(NodeId::build(6, 0, 0, 0), &[]), None);

        assert_eq!(router_a.closest_node(NodeId::build(0, 1, 0, 1), &[]), None);
        assert_eq!(router_a.closest_node(NodeId::build(0, 6, 0, 2), &[]), Some((conn_0002, node_0002, 0, 2)));
        assert_eq!(router_a.closest_node(NodeId::build(2, 1, 0, 3), &[]), Some((conn_0003, node_0003, 0, 3)));
        assert_eq!(router_a.closest_node(NodeId::build(2, 6, 0, 4), &[]), None);
    }

    #[test]
    fn random_test_closest() {
        //TODO
    }

    #[test]
    fn to_key_consistency() {
        for _ in 0..100 {
            let (node_a, _conn_a, mut router_a) = create_router(rand::random());
            let (node_b, _conn_b, mut router_b) = create_router(rand::random());

            router_a.set_direct(_conn_a, Metric::new(1, vec![node_b], 1));
            router_b.set_direct(_conn_b, Metric::new(1, vec![node_a], 1));

            for i in 0..100 {
                let key: u32 = rand::random();
                let next_a = router_a.closest_node(key, &vec![]);
                let next_b = router_b.closest_node(key, &vec![]);
                match (next_a, next_b) {
                    (Some(a), Some(b)) => {
                        panic!("Step {}, Wrong with key {} => {:?} {:?}", i, key, a, b);
                    }
                    (None, None) => {
                        panic!("Step {}, Wrong with key {} => None None", i, key);
                    }
                    _ => {}
                }
            }
        }
    }
}

use bluesea_identity::{ConnId, NodeId, NodeIdType};
use serde::{Deserialize, Serialize};

use crate::registry::{Registry, RegistrySync};
use crate::table::{Metric, Path, Table, TableSync};
use crate::{ServiceDestination};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RouterSync(pub(crate) RegistrySync, pub(crate) [Option<TableSync>; 4]);

pub struct Router {
    local_node_id: NodeId,
    tables: [Table; 4],
    service_registry: Registry,
}

impl Router {
    pub fn new(local_node_id: NodeId) -> Self {
        let tables = [
            Table::new(local_node_id, 0),
            Table::new(local_node_id, 1),
            Table::new(local_node_id, 2),
            Table::new(local_node_id, 3),
        ];

        Router {
            local_node_id,
            tables,
            service_registry: Registry::new(local_node_id),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.local_node_id
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

    pub fn service_next(
        &self,
        service_id: u8,
        excepts: &Vec<NodeId>,
    ) -> Option<ServiceDestination> {
        self.service_registry.next(service_id, excepts)
    }

    pub fn set_direct(&mut self, over: ConnId, over_node: NodeId, metric: Metric) {
        let eq_util_layer = self.local_node_id.eq_util_layer(&over_node) as usize;
        log::debug!(
            "[Router {}] set_direct {}/{} with metric {:?}, eq_util_layer {}",
            self.local_node_id,
            over,
            over_node,
            metric,
            eq_util_layer
        );
        assert!(eq_util_layer <= 4);
        debug_assert!(eq_util_layer > 0);
        if eq_util_layer > 0 {
            self.tables[eq_util_layer - 1].add_direct(over, over_node, metric.clone());
        }
    }

    pub fn del_direct(&mut self, over: ConnId) {
        log::debug!("[Router {}] del_direct {}", self.local_node_id, over);
        for table in &mut self.tables {
            table.del_direct(over);
        }
        self.service_registry.del_direct(over);
    }

    pub fn next(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<(ConnId, NodeId)> {
        let eq_util_layer = self.local_node_id.eq_util_layer(&dest) as usize;
        debug_assert!(eq_util_layer <= 4);
        if eq_util_layer == 0 {
            None
        } else {
            self.tables
                .get(eq_util_layer - 1)?
                .next(dest, excepts)
        }
    }

    pub fn next_path(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<Path> {
        let eq_util_layer = self.local_node_id.eq_util_layer(&dest) as usize;
        debug_assert!(eq_util_layer <= 4);
        if eq_util_layer == 0 {
            None
        } else {
            self.tables
                .get(eq_util_layer - 1)?
                .next_path(dest, excepts)
        }
    }

    pub fn closest_node(&self, key: NodeId, excepts: &Vec<NodeId>) -> Option<(ConnId, NodeId, u8, u8)> {
        for i in [3, 2, 1, 0] {
            let index = key.layer(i);
            let (mut next_index, next_conn, next_node) = self.tables[i as usize].closest_for(index, excepts)?;
            let current_node_index = self.local_node_id.layer(i);
            let next_distance = index ^ next_index;
            let local_distance = index ^ current_node_index;
            if local_distance < next_distance { //if current node is closer => set to current index
                next_index = current_node_index;
            }

            if next_index != current_node_index { //other zone => sending this this
                log::debug!("[Router {}] next node for {} ({}) in layer {}: {:?} => ({}, {:?})", self.local_node_id, key, index, i, self.tables[i as usize].slots(), next_index, next_node);
                return Some((next_conn, next_node, i, next_index));
            } else {
                log::debug!("[Router {}] finding closest to {} in layer {} {:?} -> ({}, {:?})", self.local_node_id, key, i, self.tables[i as usize].slots(), next_index, next_node);
            }
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

    pub fn apply_sync(
        &mut self,
        conn: ConnId,
        src: NodeId,
        src_send_metric: Metric,
        sync: RouterSync,
    ) {
        self.service_registry
            .apply_sync(conn, src, src_send_metric.clone(), sync.0);
        for (index, table_sync) in sync.1.into_iter().enumerate() {
            if let Some(table_sync) = table_sync {
                self.tables[index].apply_sync(conn, src, src_send_metric.clone(), table_sync);
            }
        }
    }

    pub fn dump(&self) {
        self.service_registry.dump();
        log::info!("[Router {}] dump begin", self.local_node_id);
        for i in 0..4 {
            self.tables[3 - i].dump();
        }
        log::info!("[Router {}] dump end", self.local_node_id);
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::registry::{RegistrySync, REGISTRY_LOCAL_BW};
    use crate::router::{Router, RouterSync};
    use crate::table::{Metric, Path, TableSync};
    use crate::ServiceDestination;
    use bluesea_identity::{ConnDirection, ConnId, NodeId, NodeIdType};

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

        router.set_direct(
            z_node1_conn,
            z_node1,
            Metric::new(1, vec![z_node1, node0], 1),
        );
        router.set_direct(node1_conn, node1, Metric::new(1, vec![node1, node0], 1));

        //same zone, group
        assert_eq!(
            router.next(node1, &vec![node0]),
            Some((node1_conn, node1))
        );
        assert_eq!(router.next(node2, &vec![node0]), None);

        //other zone
        assert_eq!(
            router.next(z_node2, &vec![node0]),
            Some((z_node1_conn, z_node1))
        );
    }

    fn create_router(node_id: NodeId) -> (NodeId, ConnId, Router) {
        (
            node_id,
            ConnId::from_out(0, node_id as u64),
            Router::new(node_id),
        )
    }

    #[test]
    fn complex_sync_same_zone() {
        /**
        A -1- B -1- C -1- F
        |1    |1
        D -2- E
        **/
        let (node_a, conn_a, mut router_a) = create_router(0x01);
        let (node_b, conn_b, mut router_b) = create_router(0x02);
        let (node_c, conn_c, mut router_c) = create_router(0x03);
        let (node_d, conn_d, mut router_d) = create_router(0x04);
        let (node_e, conn_e, mut router_e) = create_router(0x05);
        let (node_f, conn_f, mut router_f) = create_router(0x06);

        let metric_registry_local = Metric::new(0, vec![node_a], REGISTRY_LOCAL_BW);

        router_a.register_service(1);
        router_a.set_direct(conn_d, node_d, Metric::new(1, vec![node_d, node_a], 1));
        router_a.set_direct(conn_b, node_b, Metric::new(1, vec![node_b, node_a], 1));

        router_b.set_direct(conn_a, node_a, Metric::new(1, vec![node_a, node_b], 1));
        router_b.set_direct(conn_c, node_c, Metric::new(1, vec![node_c, node_b], 1));
        router_b.set_direct(conn_e, node_e, Metric::new(1, vec![node_e, node_b], 1));

        router_c.set_direct(conn_b, node_b, Metric::new(1, vec![node_b, node_c], 1));
        router_c.set_direct(conn_f, node_f, Metric::new(1, vec![node_f, node_c], 1));

        router_d.set_direct(conn_a, node_a, Metric::new(1, vec![node_a, node_d], 1));
        router_d.set_direct(conn_e, node_e, Metric::new(2, vec![node_e, node_d], 2));

        router_e.set_direct(conn_d, node_d, Metric::new(2, vec![node_d, node_e], 2));
        router_e.set_direct(conn_b, node_b, Metric::new(1, vec![node_b, node_e], 1));

        router_f.set_direct(conn_c, node_c, Metric::new(1, vec![node_c, node_f], 1));

        let empty_sync = TableSync(Vec::new());

        //create sync
        let sync_a_b = router_a.create_sync(node_b);
        assert_eq!(
            sync_a_b,
            RouterSync(
                RegistrySync(vec![(1, metric_registry_local.clone())]),
                [
                    Some(TableSync(vec![(
                        4,
                        Metric::new(1, vec![node_d, node_a], 1)
                    )])),
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
            router_b.apply_sync(
                conn_a,
                node_a,
                Metric::new(1, vec![node_a, node_b], 1),
                sync_a_b,
            );
            router_d.apply_sync(
                conn_a,
                node_a,
                Metric::new(1, vec![node_a, node_d], 1),
                sync_a_d,
            );

            //for node B
            let sync_b_a = router_b.create_sync(node_a);
            let sync_b_c = router_b.create_sync(node_c);
            let sync_b_e = router_b.create_sync(node_e);
            router_a.apply_sync(
                conn_b,
                node_b,
                Metric::new(1, vec![node_b, node_a], 1),
                sync_b_a,
            );
            router_c.apply_sync(
                conn_b,
                node_b,
                Metric::new(1, vec![node_b, node_c], 1),
                sync_b_c,
            );
            router_e.apply_sync(
                conn_b,
                node_b,
                Metric::new(1, vec![node_b, node_e], 1),
                sync_b_e,
            );

            //for node C
            let sync_c_b = router_c.create_sync(node_b);
            let sync_c_f = router_c.create_sync(node_f);
            router_b.apply_sync(
                conn_c,
                node_c,
                Metric::new(1, vec![node_c, node_b], 1),
                sync_c_b,
            );
            router_f.apply_sync(
                conn_c,
                node_c,
                Metric::new(1, vec![node_c, node_f], 1),
                sync_c_f,
            );

            //for node D
            let sync_d_a = router_d.create_sync(node_a);
            let sync_d_e = router_d.create_sync(node_e);
            router_a.apply_sync(
                conn_d,
                node_d,
                Metric::new(1, vec![node_d, node_a], 1),
                sync_d_a,
            );
            router_e.apply_sync(
                conn_d,
                node_d,
                Metric::new(2, vec![node_d, node_e], 2),
                sync_d_e,
            );

            //for node E
            let sync_e_b = router_e.create_sync(node_b);
            let sync_e_d = router_e.create_sync(node_d);
            router_b.apply_sync(
                conn_e,
                node_e,
                Metric::new(1, vec![node_e, node_b], 1),
                sync_e_b,
            );
            router_d.apply_sync(
                conn_e,
                node_e,
                Metric::new(2, vec![node_e, node_d], 2),
                sync_e_d,
            );

            //for node F
            let sync_f_c = router_f.create_sync(node_c);
            router_c.apply_sync(
                conn_f,
                node_f,
                Metric::new(1, vec![node_f, node_c], 1),
                sync_f_c,
            );
        }

        /**
        A -1- B -1- C -1- F
        |1    |1
        D -2- E
        **/

        assert_eq!(
            router_a.next_path(node_b, &vec![]),
            Some(Path(
                conn_b,
                node_b,
                Metric::new(1, vec![node_b, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_c, &vec![]),
            Some(Path(
                conn_b,
                node_b,
                Metric::new(2, vec![node_c, node_b, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_d, &vec![]),
            Some(Path(
                conn_d,
                node_d,
                Metric::new(1, vec![node_d, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_e, &vec![]),
            Some(Path(
                conn_b,
                node_b,
                Metric::new(2, vec![node_e, node_b, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_f, &vec![]),
            Some(Path(
                conn_b,
                node_b,
                Metric::new(3, vec![node_f, node_c, node_b, node_a], 1)
            ))
        );
        assert_eq!(
            router_f.service_next(1, &vec![]),
            Some(ServiceDestination::Remote(conn_c, node_c))
        );
        assert_eq!(
            router_c.service_next(1, &vec![]),
            Some(ServiceDestination::Remote(conn_b, node_b))
        );
        assert_eq!(
            router_b.service_next(1, &vec![]),
            Some(ServiceDestination::Remote(conn_a, node_a))
        );
        assert_eq!(
            router_e.service_next(1, &vec![]),
            Some(ServiceDestination::Remote(conn_b, node_b))
        );
        assert_eq!(
            router_d.service_next(1, &vec![]),
            Some(ServiceDestination::Remote(conn_a, node_a))
        );

        /** remove A - B
        A     B -1- C -1- F
        |1    |1
        D -2- E
        **/
        router_a.del_direct(conn_b);
        router_b.del_direct(conn_a);

        for _i in 0..4 {
            //for node A
            let sync_a_d = router_a.create_sync(node_d);
            router_d.apply_sync(
                conn_a,
                node_a,
                Metric::new(1, vec![node_a, node_d], 1),
                sync_a_d,
            );

            //for node B
            let sync_b_c = router_b.create_sync(node_c);
            let sync_b_e = router_b.create_sync(node_e);
            router_c.apply_sync(
                conn_b,
                node_b,
                Metric::new(1, vec![node_b, node_c], 1),
                sync_b_c,
            );
            router_e.apply_sync(
                conn_b,
                node_b,
                Metric::new(1, vec![node_b, node_e], 1),
                sync_b_e,
            );

            //for node C
            let sync_c_b = router_c.create_sync(node_b);
            let sync_c_f = router_c.create_sync(node_f);
            router_b.apply_sync(
                conn_c,
                node_c,
                Metric::new(1, vec![node_c, node_b], 1),
                sync_c_b,
            );
            router_f.apply_sync(
                conn_c,
                node_c,
                Metric::new(1, vec![node_c, node_f], 1),
                sync_c_f,
            );

            //for node D
            let sync_d_a = router_d.create_sync(node_a);
            let sync_d_e = router_d.create_sync(node_e);
            router_a.apply_sync(
                conn_d,
                node_d,
                Metric::new(1, vec![node_d, node_a], 1),
                sync_d_a,
            );
            router_e.apply_sync(
                conn_d,
                node_d,
                Metric::new(2, vec![node_d, node_e], 2),
                sync_d_e,
            );

            //for node E
            let sync_e_b = router_e.create_sync(node_b);
            let sync_e_d = router_e.create_sync(node_d);
            router_b.apply_sync(
                conn_e,
                node_e,
                Metric::new(1, vec![node_e, node_b], 1),
                sync_e_b,
            );
            router_d.apply_sync(
                conn_e,
                node_e,
                Metric::new(2, vec![node_e, node_d], 2),
                sync_e_d,
            );

            //for node F
            let sync_f_c = router_f.create_sync(node_c);
            router_c.apply_sync(
                conn_f,
                node_f,
                Metric::new(1, vec![node_f, node_c], 1),
                sync_f_c,
            );
        }

        /** remove A - B
        A     B -1- C -1- F
        |1    |1
        D -2- E
        **/
        assert_eq!(
            router_a.next_path(node_b, &vec![node_a]),
            Some(Path(
                conn_d,
                node_d,
                Metric::new(4, vec![node_b, node_e, node_d, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_c, &vec![node_a]),
            Some(Path(
                conn_d,
                node_d,
                Metric::new(5, vec![node_c, node_b, node_e, node_d, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_d, &vec![node_a]),
            Some(Path(
                conn_d,
                node_d,
                Metric::new(1, vec![node_d, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_e, &vec![node_a]),
            Some(Path(
                conn_d,
                node_d,
                Metric::new(3, vec![node_e, node_d, node_a], 1)
            ))
        );
        assert_eq!(
            router_a.next_path(node_f, &vec![node_a]),
            Some(Path(
                conn_d,
                node_d,
                Metric::new(6, vec![node_f, node_c, node_b, node_e, node_d, node_a], 1)
            ))
        );
    }

    #[test]
    fn closest_node() {
        let (node_A, conn_A, mut router_A) = create_router(NodeId::build(0, 0, 0, 1));
    
        assert_eq!(router_A.closest_node(0x01, &vec![]), None);
    
        let node_5000 = NodeId::build(5,0,0,1);
        let node_0500 = NodeId::build(0,5,0,1);

        let conn_5000 = ConnId::from_out(0, 5000);
        let conn_0500 = ConnId::from_out(0, 500);

        router_A.set_direct(conn_5000, node_5000, Metric::new(1,vec![node_5000, node_A],1));
        router_A.set_direct(conn_0500, node_0500, Metric::new(1,vec![node_0500, node_A],1));
    
        assert_eq!(router_A.closest_node(NodeId::build(1, 0, 0, 0), &vec![]), None);
        assert_eq!(router_A.closest_node(NodeId::build(4, 0, 0, 0), &vec![]), Some((conn_5000, node_5000, 3, 5)));
        assert_eq!(router_A.closest_node(NodeId::build(6, 0, 0, 0), &vec![]), Some((conn_5000, node_5000, 3, 5)));
    
        assert_eq!(router_A.closest_node(NodeId::build(0, 1, 0, 0), &vec![]), None);
        assert_eq!(router_A.closest_node(NodeId::build(0, 6, 0, 0), &vec![]), Some((conn_0500, node_0500, 2, 5)));
        assert_eq!(router_A.closest_node(NodeId::build(2, 1, 0, 0), &vec![]), None);
        assert_eq!(router_A.closest_node(NodeId::build(2, 6, 0, 0), &vec![]), Some((conn_0500, node_0500, 2, 5)));
    }

    #[test]
    fn random_test_closest() {
        //TODO
    }
}

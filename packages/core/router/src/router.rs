use bluesea_identity::{NodeId, NodeIdType};
use serde::{Deserialize, Serialize};

use crate::registry::{Registry, RegistrySync};
use crate::table::{Metric, Path, Table, TableSync};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RouterSync(pub(crate) RegistrySync, pub(crate) [Option<TableSync>; 4]);

pub struct Router {
    id: NodeId,
    tables: [Table; 4],
    service_registry: Registry,
}

impl Router {
    pub fn new(id: NodeId) -> Self {
        let tables = [Table::new(id, 0), Table::new(id, 1), Table::new(id, 2), Table::new(id, 3)];

        Router {
            id,
            tables,
            service_registry: Registry::new(id),
        }
    }

    pub fn node_id(&self) -> NodeId {
        self.id
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

    pub fn service_next(&self, service_id: u8, excepts: &Vec<NodeId>) -> Option<NodeId> {
        self.service_registry.next(service_id, excepts)
    }

    pub fn set_direct(&mut self, over: NodeId, metric: Metric) {
        let eq_util_layer = self.id.eq_util_layer(&over) as usize;
        log::debug!("[Router {}] set_direct {} with metric {:?}, eq_util_layer {}", self.id, over, metric, eq_util_layer);
        assert!(eq_util_layer <= 4);
        debug_assert!(eq_util_layer > 0);
        if eq_util_layer > 0 {
            self.tables[eq_util_layer - 1].add_direct(over, metric.clone());
        }
    }

    pub fn del_direct(&mut self, over: NodeId) {
        log::debug!("[Router {}] del_direct {}", self.id, over);
        for table in &mut self.tables {
            table.del_direct(over);
        }
        self.service_registry.del_direct(over);
    }

    pub fn next(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<NodeId> {
        let eq_util_layer = self.id.eq_util_layer(&dest) as usize;
        debug_assert!(eq_util_layer <= 4);
        if eq_util_layer == 0 {
            Some(self.id)
        } else {
            self.tables.get(eq_util_layer - 1)?.next(dest, excepts)
        }
    }

    pub fn next_path(&self, dest: NodeId, excepts: &Vec<NodeId>) -> Option<Path> {
        let eq_util_layer = self.id.eq_util_layer(&dest) as usize;
        debug_assert!(eq_util_layer <= 4);
        if eq_util_layer == 0 {
            Some(Path(self.id, Metric::new(0, vec![self.id], 0)))
        } else {
            self.tables.get(eq_util_layer - 1)?.next_path(dest, excepts)
        }
    }

    // pub fn closest_node(&self, key: NodeId, excepts: &Vec<NodeId>) -> Option<NodeId> {
    //     for i in [3, 2, 1, 0] {
    //         let index = key.layer(i);
    //         let current_node_index = self.id.layer(i);
    //         let (next_index, next_node) = self.tables[i as usize].closest_for(index, excepts);
    //
    //         if next_index != current_node_index { //other zone => sending this this
    //         log::debug!("[Router {}] next node for {} ({}) in layer {}: {:?} => ({}, {:?})", self.id, key, index, i, self.tables[i as usize].slots(), next_index, next_node);
    //             return next_node;
    //         } else {
    //             log::debug!("[Router {}] finding closest to {} in layer {} {:?} -> ({}, {:?})", self.id, key, i, self.tables[i as usize].slots(), next_index, next_node);
    //         }
    //     }
    //
    //     Some(self.id)
    // }

    pub fn create_sync(&self, for_node: NodeId) -> RouterSync {
        RouterSync(
            self.service_registry.sync_for(for_node),
            [
                self.tables[0].sync_for(for_node),
                self.tables[1].sync_for(for_node),
                self.tables[2].sync_for(for_node),
                self.tables[3].sync_for(for_node)
            ]
        )
    }

    pub fn apply_sync(&mut self, src: NodeId, src_send_metric: Metric, sync: RouterSync) {
        self.service_registry.apply_sync(src, src_send_metric.clone(), sync.0);
        for (index, table_sync) in sync.1.into_iter().enumerate() {
            if let Some(table_sync) = table_sync {
                self.tables[index].apply_sync(src, src_send_metric.clone(), table_sync);
            }
        }
    }

    pub fn dump(&self) {
        self.service_registry.dump();
        log::info!("[Router {}] dump begin", self.id);
        for i in 0..4 {
            self.tables[3 - i].dump();
        }
        log::info!("[Router {}] dump end", self.id);
    }
}

#[cfg(test)]
mod tests {
    use bluesea_identity::{NodeId, NodeIdType};
    use crate::router::{Router, RouterSync};
    use crate::registry::{REGISTRY_LOCAL_BW, RegistrySync};
    use crate::table::{Metric, Path, TableSync};

    #[test]
    fn create_manual_multi_layers() {
        let node0: NodeId = 0x0;
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let _node3: NodeId = 0x3;
        let z_node1: NodeId = 0x01000001;
        let z_node2: NodeId = 0x01000002;

        let mut router = Router::new(node0);

        router.set_direct(z_node1, Metric::new(1, vec![z_node1, node0], 1));
        router.set_direct(node1, Metric::new(1, vec![node1, node0], 1));

        //same zone, group
        assert_eq!(router.next(node1, &vec![node0]), Some(node1));
        assert_eq!(router.next(node2, &vec![node0]), None);

        //other zone
        assert_eq!(router.next(z_node2, &vec![node0]), Some(z_node1));
    }

    fn create_router(node_id: NodeId) -> (NodeId, Router) {
        (node_id, Router::new(node_id))
    }

    #[test]
    fn complex_sync_same_zone() {
        /**
        A -1- B -1- C -1- F
        |1    |1
        D -2- E
        **/

        let (node_A, mut router_A) = create_router(0x01);
        let (node_B, mut router_B) = create_router(0x02);
        let (node_C, mut router_C) = create_router(0x03);
        let (node_D, mut router_D) = create_router(0x04);
        let (node_E, mut router_E) = create_router(0x05);
        let (node_F, mut router_F) = create_router(0x06);

        let metric_registry_local = Metric::new(0,vec![node_A], REGISTRY_LOCAL_BW);

        router_A.register_service(1);
        router_A.set_direct(node_D, Metric::new(1,vec![node_D, node_A],1));
        router_A.set_direct(node_B, Metric::new(1,vec![node_B, node_A],1));

        router_B.set_direct(node_A, Metric::new(1,vec![node_A, node_B],1));
        router_B.set_direct(node_C, Metric::new(1,vec![node_C, node_B],1));
        router_B.set_direct(node_E, Metric::new(1,vec![node_E, node_B],1));

        router_C.set_direct(node_B, Metric::new(1,vec![node_B, node_C],1));
        router_C.set_direct(node_F, Metric::new(1,vec![node_F, node_C],1));

        router_D.set_direct(node_A, Metric::new(1,vec![node_A, node_D],1));
        router_D.set_direct(node_E, Metric::new(2,vec![node_E, node_D],2));

        router_E.set_direct(node_D, Metric::new(2,vec![node_D, node_E],2));
        router_E.set_direct(node_B, Metric::new(1,vec![node_B, node_E],1));

        router_F.set_direct(node_C, Metric::new(1,vec![node_C, node_F],1));

        let empty_sync = TableSync(Vec::new());

        //create sync
        let sync_A_B = router_A.create_sync(node_B);
        assert_eq!(sync_A_B, RouterSync(
            RegistrySync(vec![(1, metric_registry_local.clone())]),
            [
                Some(TableSync(vec![(4, Metric::new(1,vec![node_D, node_A],1))])),
                Some(empty_sync.clone()),
                Some(empty_sync.clone()),
                Some(empty_sync.clone())
            ]));

        for _i in 0..4 {
            //for node A
            let sync_A_B = router_A.create_sync(node_B);
            let sync_A_D = router_A.create_sync(node_D);
            router_B.apply_sync(node_A, Metric::new(1,vec![node_A, node_B],1), sync_A_B);
            router_D.apply_sync(node_A, Metric::new(1,vec![node_A, node_D],1), sync_A_D);

            //for node B
            let sync_B_A = router_B.create_sync(node_A);
            let sync_B_C = router_B.create_sync(node_C);
            let sync_B_E = router_B.create_sync(node_E);
            router_A.apply_sync(node_B, Metric::new(1,vec![node_B, node_A],1), sync_B_A);
            router_C.apply_sync(node_B, Metric::new(1,vec![node_B, node_C],1), sync_B_C);
            router_E.apply_sync(node_B, Metric::new(1,vec![node_B, node_E],1), sync_B_E);

            //for node C
            let sync_C_B = router_C.create_sync(node_B);
            let sync_C_F = router_C.create_sync(node_F);
            router_B.apply_sync(node_C, Metric::new(1,vec![node_C, node_B],1), sync_C_B);
            router_F.apply_sync(node_C, Metric::new(1,vec![node_C, node_F],1), sync_C_F);

            //for node D
            let sync_D_A = router_D.create_sync(node_A);
            let sync_D_E = router_D.create_sync(node_E);
            router_A.apply_sync(node_D, Metric::new(1,vec![node_D, node_A],1), sync_D_A);
            router_E.apply_sync(node_D, Metric::new(2,vec![node_D, node_E],2), sync_D_E);

            //for node E
            let sync_E_B = router_E.create_sync(node_B);
            let sync_E_D = router_E.create_sync(node_D);
            router_B.apply_sync(node_E, Metric::new(1,vec![node_E, node_B],1), sync_E_B);
            router_D.apply_sync(node_E, Metric::new(2,vec![node_E, node_D],2), sync_E_D);

            //for node F
            let sync_F_C = router_F.create_sync(node_C);
            router_C.apply_sync(node_F, Metric::new(1,vec![node_F, node_C],1), sync_F_C);
        }

        /**
        A -1- B -1- C -1- F
        |1    |1
        D -2- E
        **/

        assert_eq!(router_A.next_path(node_B, &vec![]), Some(Path(node_B, Metric::new(1, vec![node_B, node_A], 1))));
        assert_eq!(router_A.next_path(node_C, &vec![]), Some(Path(node_B, Metric::new(2, vec![node_C, node_B, node_A], 1))));
        assert_eq!(router_A.next_path(node_D, &vec![]), Some(Path(node_D, Metric::new(1, vec![node_D, node_A], 1))));
        assert_eq!(router_A.next_path(node_E, &vec![]), Some(Path(node_B, Metric::new(2, vec![node_E, node_B, node_A], 1))));
        assert_eq!(router_A.next_path(node_F, &vec![]), Some(Path(node_B, Metric::new(3, vec![node_F, node_C, node_B, node_A], 1))));
        assert_eq!(router_F.service_next(1, &vec![]), Some(node_C));
        assert_eq!(router_C.service_next(1, &vec![]), Some(node_B));
        assert_eq!(router_B.service_next(1, &vec![]), Some(node_A));
        assert_eq!(router_E.service_next(1, &vec![]), Some(node_B));
        assert_eq!(router_D.service_next(1, &vec![]), Some(node_A));

        /** remove A - B
               A     B -1- C -1- F
               |1    |1
               D -2- E
               **/
        router_A.del_direct(node_B);
        router_B.del_direct(node_A);

        for _i in 0..4 {
            //for node A
            let sync_A_D = router_A.create_sync(node_D);
            router_D.apply_sync(node_A, Metric::new(1,vec![node_A, node_D],1), sync_A_D);

            //for node B
            let sync_B_C = router_B.create_sync(node_C);
            let sync_B_E = router_B.create_sync(node_E);
            router_C.apply_sync(node_B, Metric::new(1,vec![node_B, node_C],1), sync_B_C);
            router_E.apply_sync(node_B, Metric::new(1,vec![node_B, node_E],1), sync_B_E);

            //for node C
            let sync_C_B = router_C.create_sync(node_B);
            let sync_C_F = router_C.create_sync(node_F);
            router_B.apply_sync(node_C, Metric::new(1,vec![node_C, node_B],1), sync_C_B);
            router_F.apply_sync(node_C, Metric::new(1,vec![node_C, node_F],1), sync_C_F);

            //for node D
            let sync_D_A = router_D.create_sync(node_A);
            let sync_D_E = router_D.create_sync(node_E);
            router_A.apply_sync(node_D, Metric::new(1,vec![node_D, node_A],1), sync_D_A);
            router_E.apply_sync(node_D, Metric::new(2,vec![node_D, node_E],2), sync_D_E);

            //for node E
            let sync_E_B = router_E.create_sync(node_B);
            let sync_E_D = router_E.create_sync(node_D);
            router_B.apply_sync(node_E, Metric::new(1,vec![node_E, node_B],1), sync_E_B);
            router_D.apply_sync(node_E, Metric::new(2,vec![node_E, node_D],2), sync_E_D);

            //for node F
            let sync_F_C = router_F.create_sync(node_C);
            router_C.apply_sync(node_F, Metric::new(1,vec![node_F, node_C],1), sync_F_C);
        }

        /** remove A - B
               A     B -1- C -1- F
               |1    |1
               D -2- E
               **/
        assert_eq!(router_A.next_path(node_B, &vec![node_A]), Some(Path(node_D, Metric::new(4, vec![node_B, node_E, node_D, node_A], 1))));
        assert_eq!(router_A.next_path(node_C, &vec![node_A]), Some(Path(node_D, Metric::new(5, vec![node_C, node_B, node_E, node_D, node_A], 1))));
        assert_eq!(router_A.next_path(node_D, &vec![node_A]), Some(Path(node_D, Metric::new(1, vec![node_D, node_A], 1))));
        assert_eq!(router_A.next_path(node_E, &vec![node_A]), Some(Path(node_D, Metric::new(3, vec![node_E, node_D, node_A], 1))));
        assert_eq!(router_A.next_path(node_F, &vec![node_A]), Some(Path(node_D, Metric::new(6, vec![node_F, node_C, node_B, node_E, node_D, node_A], 1))));
    }

    // #[test]
    // fn closest_node() {
    //     let (node_A, mut router_A) = create_router(NodeId::build(0, 0, 0, 1));
    //
    //     assert_eq!(router_A.closest_node(0x01, &vec![]), Some(node_A));
    //
    //     let node_5000 = NodeId::build(5,0,0,1);
    //     let node_0500 = NodeId::build(0,5,0,1);
    //     router_A.set_direct(node_5000, Metric::new(1,vec![node_5000, node_A],1));
    //     router_A.set_direct(node_0500, Metric::new(1,vec![node_0500, node_A],1));
    //
    //     assert_eq!(router_A.closest_node(NodeId::build(1, 0, 0, 0), &vec![]), Some(node_A));
    //     assert_eq!(router_A.closest_node(NodeId::build(4, 0, 0, 0), &vec![]), Some(node_5000));
    //     assert_eq!(router_A.closest_node(NodeId::build(6, 0, 0, 0), &vec![]), Some(node_5000));
    //
    //     assert_eq!(router_A.closest_node(NodeId::build(0, 1, 0, 0), &vec![]), Some(node_A));
    //     assert_eq!(router_A.closest_node(NodeId::build(0, 6, 0, 0), &vec![]), Some(node_0500));
    //     assert_eq!(router_A.closest_node(NodeId::build(2, 1, 0, 0), &vec![]), Some(node_A));
    //     assert_eq!(router_A.closest_node(NodeId::build(2, 6, 0, 0), &vec![]), Some(node_0500));
    // }
    //
    // #[test]
    // fn closest_node_strong() {
    //     let (node_A, mut router_A) = create_router(1);
    //     let (node_B, mut router_B) = create_router(16842753);
    //
    //     router_A.set_direct(node_B, Metric::new(1,vec![node_B, node_A],1));
    //     router_A.set_direct(16842752, Metric::new(1,vec![16842752, node_A],1));
    //
    //     router_B.set_direct(node_A, Metric::new(1,vec![node_A, node_B],1));
    //     router_B.set_direct(16842752, Metric::new(1,vec![16842752, node_B],1));
    //
    //     assert_eq!(router_A.closest_node(2181103616, &vec![]), Some(node_A));
    //     assert_eq!(router_B.closest_node(2181103616, &vec![]), Some(node_A));
    // }
}
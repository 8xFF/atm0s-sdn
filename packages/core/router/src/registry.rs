use bluesea_identity::{ConnId, NodeId};
use std::collections::HashMap;

use crate::ServiceDestination;
use serde::{Deserialize, Serialize};
use utils::init_array::init_array;

use crate::table::{Dest, Metric, Path};

pub const REGISTRY_LOCAL_BW: u32 = 1000000; //1Gbps

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct RegistrySync(pub Vec<(u8, Metric)>);

pub struct Registry {
    node_id: NodeId,
    local_destinations: [bool; 256],
    remote_destinations: [Dest; 256],
}

impl Registry {
    pub fn new(node_id: NodeId) -> Self {
        Registry {
            node_id,
            local_destinations: init_array!(bool, 256, false),
            remote_destinations: init_array!(Dest, 256, Default::default()),
        }
    }

    pub fn add_service(&mut self, service_id: u8) {
        self.local_destinations[service_id as usize] = true;
    }

    pub fn remove_service(&mut self, service_id: u8) {
        self.local_destinations[service_id as usize] = false;
    }

    pub fn del_direct(&mut self, conn: ConnId) {
        for i in 0..256 {
            let pre_empty = self.remote_destinations[i as usize].is_empty();
            self.remote_destinations[i as usize].del_path(conn);
            if !pre_empty && self.remote_destinations[i as usize].is_empty() {
                log::info!(
                    "[Registry] removed service {} from dest {} because of direct disconnected",
                    i,
                    conn
                );
            }
        }
    }

    pub fn next(&self, service_id: u8, excepts: &Vec<NodeId>) -> Option<ServiceDestination> {
        if self.local_destinations[service_id as usize] {
            Some(ServiceDestination::Local)
        } else {
            self.remote_destinations[service_id as usize]
                .next(excepts)
                .map(|(c, n)| ServiceDestination::Remote(c, n))
        }
    }

    pub fn apply_sync(
        &mut self,
        src_conn: ConnId,
        src: NodeId,
        src_send_metric: Metric,
        sync: RegistrySync,
    ) {
        log::debug!(
            "apply sync from {} -> {}, sync {:?}",
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
            let dest: &mut Dest = &mut self.remote_destinations[i as usize];
            match cached.remove(&i) {
                None => {
                    if !dest.is_empty() {
                        if let Some(_) = dest.del_path(src_conn) {
                            log::info!(
                                "[Registry] removed service {} from dest {} after sync",
                                i,
                                src
                            );
                        }
                    }
                }
                Some(metric) => {
                    if dest.is_empty() {
                        log::info!("[Registry] added service {} from {} after sync", i, src);
                    }
                    dest.set_path(src_conn, src, metric);
                }
            }
        }
    }

    pub fn sync_for(&self, node: NodeId) -> RegistrySync {
        let mut res = vec![];
        for i in 0..=255 {
            if self.local_destinations[i as usize] {
                res.push((i, Metric::new(0, vec![self.node_id], REGISTRY_LOCAL_BW)));
            } else {
                let dest: &Dest = &self.remote_destinations[i as usize];
                if !dest.is_empty() {
                    if let Some(Path(_over, _over_node, metric)) = dest.best_for(node) {
                        res.push((i, metric));
                    }
                }
            }
        }
        RegistrySync(res)
    }

    pub fn dump(&self) {
        let mut local_services = vec![];
        let mut index = 0;
        for service_id in &self.local_destinations {
            if *service_id {
                local_services.push(index);
            }
            index += 1;
        }

        let mut slots = vec![];
        let mut nexts = vec![];
        let mut index = 0;
        for dest in &self.remote_destinations {
            if !dest.is_empty() {
                slots.push(index);
                nexts.push(dest.next(&vec![]));
            }
            index += 1;
        }
        log::info!(
            "[Registry {}] local services: {:?} remote services: {:?}, nexts {:?}",
            self.node_id,
            local_services,
            slots,
            nexts
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::registry::{Registry, RegistrySync, REGISTRY_LOCAL_BW};
    use crate::table::Metric;
    use crate::ServiceDestination;
    use bluesea_identity::{ConnDirection, ConnId, NodeId};

    #[test]
    fn create_manual() {
        let node0: NodeId = 0x0;
        let mut registry = Registry::new(node0);
        let node1: NodeId = 0x1;
        let _node2: NodeId = 0x2;
        let _node3: NodeId = 0x3;

        registry.add_service(1);

        assert_eq!(registry.next(1, &vec![]), Some(ServiceDestination::Local));
        // assert_eq!(registry.next(1, &vec![0]), None);

        let sync = registry.sync_for(node1);
        assert_eq!(
            sync.0,
            vec![(1, Metric::new(0, vec![0], REGISTRY_LOCAL_BW))]
        );
    }

    #[test]
    fn del_direct() {
        let conn0: ConnId = ConnId::from_out(0, 0x0);
        let node0: NodeId = 0x0;

        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let mut registry = Registry::new(node0);

        assert_eq!(registry.next(1, &vec![]), None);
        registry.apply_sync(
            conn1,
            node1,
            Metric::new(1, vec![1, 0], 1),
            RegistrySync(vec![(1, Metric::new(1, vec![1], 1))]),
        );
        assert_eq!(
            registry.next(1, &vec![]),
            Some(ServiceDestination::Remote(conn1, node1))
        );

        registry.del_direct(conn1);
        assert_eq!(registry.next(1, &vec![]), None);
    }

    #[test]
    fn apply_sync() {
        let node0: NodeId = 0x0;
        let mut registry = Registry::new(node0);

        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;
        let _node2: NodeId = 0x2;
        let _node3: NodeId = 0x3;

        let sync = vec![
            (2, Metric::new(1, vec![node1], 1)),
            (3, Metric::new(1, vec![node1], 1)),
        ];
        registry.apply_sync(
            conn1,
            node1,
            Metric::new(1, vec![node1, node0], 2),
            RegistrySync(sync),
        );

        assert_eq!(registry.next(1, &vec![]), None);
        assert_eq!(
            registry.next(2, &vec![]),
            Some(ServiceDestination::Remote(conn1, node1))
        );
        assert_eq!(
            registry.next(3, &vec![]),
            Some(ServiceDestination::Remote(conn1, node1))
        );

        let sync = vec![(3, Metric::new(1, vec![node1], 1))];
        registry.apply_sync(
            conn1,
            node1,
            Metric::new(1, vec![node1, node0], 1),
            RegistrySync(sync),
        );
        assert_eq!(registry.next(1, &vec![]), None);
        assert_eq!(registry.next(2, &vec![]), None);
        assert_eq!(
            registry.next(3, &vec![]),
            Some(ServiceDestination::Remote(conn1, node1))
        );
    }

    #[test]
    fn remove_from_sync() {
        let node0: NodeId = 0x0;
        let mut registry = Registry::new(node0);
        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;
        let node2: NodeId = 0x2;
        let node3: NodeId = 0x3;
        let node4: NodeId = 0x4;

        let sync = vec![(2, Metric::new(1, vec![node3, node2, node1], 1))];
        registry.apply_sync(
            conn1,
            node1,
            Metric::new(1, vec![node1, node0], 2),
            RegistrySync(sync),
        );

        assert_eq!(
            registry.next(2, &vec![]),
            Some(ServiceDestination::Remote(conn1, node1))
        );
        assert_eq!(registry.sync_for(node1), RegistrySync(vec![]));
        assert_eq!(registry.sync_for(node2), RegistrySync(vec![]));
        assert_eq!(registry.sync_for(node3), RegistrySync(vec![]));
        assert_eq!(
            registry.sync_for(node4),
            RegistrySync(vec![(
                2,
                Metric::new(2, vec![node3, node2, node1, node0], 1)
            )])
        );
    }

    //TODO test multi connections with same node
}

use atm0s_sdn_identity::{ConnId, NodeId};
use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

mod dest;

pub use self::dest::{RegisterRemoteDestDump, RegistryRemoteDestDelta};

use super::{registry::dest::RegistryRemoteDest, Metric, Path, ServiceDestination};

#[derive(Debug, PartialEq, Clone)]
pub enum RegistryDelta {
    ServiceRemote(u8, RegistryRemoteDestDelta),
    SetServiceLocal(u8),
    DelServiceLocal(u8),
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct RegistrySync(pub Vec<(u8, Metric)>);

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct RegisterDump {
    local: Vec<u8>,
    remotes: HashMap<u8, RegisterRemoteDestDump>,
}

pub struct Registry {
    node_id: NodeId,
    local_destinations: [bool; 256],
    remote_destinations: [RegistryRemoteDest; 256],
    deltas: VecDeque<RegistryDelta>,
}

impl Registry {
    pub fn new(node_id: NodeId) -> Self {
        Registry {
            node_id,
            local_destinations: [false; 256],
            remote_destinations: std::array::from_fn(|_| RegistryRemoteDest::default()),
            deltas: VecDeque::new(),
        }
    }

    pub fn dump(&self) -> RegisterDump {
        let mut local = Vec::new();
        let mut remotes = HashMap::new();

        for i in 0..=255 {
            if self.local_destinations[i as usize] {
                local.push(i);
            }

            let dest: &RegistryRemoteDest = &self.remote_destinations[i as usize];
            if !dest.is_empty() {
                remotes.insert(i, dest.dump());
            }
        }

        RegisterDump { local, remotes }
    }

    pub fn add_service(&mut self, service_id: u8) {
        self.local_destinations[service_id as usize] = true;
        self.deltas.push_back(RegistryDelta::SetServiceLocal(service_id));
    }

    #[allow(unused)]
    pub fn remove_service(&mut self, service_id: u8) {
        self.local_destinations[service_id as usize] = false;
        self.deltas.push_back(RegistryDelta::DelServiceLocal(service_id));
    }

    pub fn del_direct(&mut self, conn: ConnId) {
        for i in 0..=255 {
            let pre_empty = self.remote_destinations[i as usize].is_empty();
            self.remote_destinations[i as usize].del_path(conn);
            if !pre_empty && self.remote_destinations[i as usize].is_empty() {
                log::info!("[Registry] removed service {} from dest {} because of direct disconnected", i, conn);
            }
            while let Some(delta) = self.remote_destinations[i as usize].pop_delta() {
                self.deltas.push_back(RegistryDelta::ServiceRemote(i, delta));
            }
        }
    }

    pub fn next(&self, service_id: u8, excepts: &[NodeId]) -> Option<ServiceDestination> {
        if self.local_destinations[service_id as usize] {
            Some(ServiceDestination::Local)
        } else {
            self.remote_destinations[service_id as usize].next(excepts).map(|(c, n)| ServiceDestination::Remote(c, n))
        }
    }

    pub fn apply_sync(&mut self, conn: ConnId, src: NodeId, metric: Metric, sync: RegistrySync) {
        log::debug!("[Registry] apply sync from {} -> {}, sync {:?}", src, self.node_id, sync.0);
        let mut cached: HashMap<u8, Metric> = HashMap::new();
        for (index, s_metric) in sync.0 {
            cached.insert(index, s_metric.add(&metric));
        }

        for i in 0..=255_u8 {
            let dest: &mut RegistryRemoteDest = &mut self.remote_destinations[i as usize];
            match cached.remove(&i) {
                None => {
                    if dest.del_path(conn).is_some() {
                        log::info!("[Registry] removed service {} from dest {} after sync", i, src);
                    }
                }
                Some(metric) => {
                    if !dest.has_path(conn) {
                        log::info!("[Registry] added service {} from {} after sync", i, src);
                    }
                    dest.set_path(conn, src, metric);
                }
            }
            while let Some(delta) = dest.pop_delta() {
                self.deltas.push_back(RegistryDelta::ServiceRemote(i, delta));
            }
        }
    }

    pub fn pop_delta(&mut self) -> Option<RegistryDelta> {
        self.deltas.pop_front()
    }

    pub fn sync_for(&self, node: NodeId) -> RegistrySync {
        let mut res = vec![];
        for i in 0..=255 {
            if self.local_destinations[i as usize] {
                res.push((i, Metric::local()));
            } else {
                let dest: &RegistryRemoteDest = &self.remote_destinations[i as usize];
                if !dest.is_empty() {
                    if let Some(path) = dest.best_for(node) {
                        res.push((i, path.metric().clone()));
                    }
                }
            }
        }
        RegistrySync(res)
    }

    pub fn log_dump(&self) {
        let mut local_services = vec![];
        for (index, service_id) in self.local_destinations.iter().enumerate() {
            if *service_id {
                local_services.push(index);
            }
        }

        let mut slots = vec![];
        for (index, dest) in self.remote_destinations.iter().enumerate() {
            if !dest.is_empty() {
                slots.push((index, dest.next(&[]).map(|(_c, n)| n)));
            }
        }
        log::debug!("[Registry {}] local services: {:?} remote services: {:?}", self.node_id, local_services, slots);
    }

    pub fn print_dump(&self) {
        let mut local_services = vec![];
        for (index, service_id) in self.local_destinations.iter().enumerate() {
            if *service_id {
                local_services.push(index);
            }
        }

        let mut slots = vec![];
        for (index, dest) in self.remote_destinations.iter().enumerate() {
            if !dest.is_empty() {
                slots.push((index, dest.next(&[]).map(|(_c, n)| n)));
            }
        }
        println!("[Registry {}] local services: {:?} remote services: {:?}", self.node_id, local_services, slots);
    }
}

#[cfg(test)]
mod tests {
    use atm0s_sdn_identity::{ConnId, NodeId};

    use crate::core::{registry::dest::RegistryRemoteDestDelta, table::BANDWIDTH_LIMIT, Metric, Registry, RegistryDelta, RegistrySync, ServiceDestination};

    #[test]
    fn create_manual() {
        let node0: NodeId = 0x0;
        let mut registry = Registry::new(node0);
        let node1: NodeId = 0x1;
        let _node2: NodeId = 0x2;
        let _node3: NodeId = 0x3;

        registry.add_service(1);
        assert_eq!(registry.pop_delta(), Some(RegistryDelta::SetServiceLocal(1)));
        assert_eq!(registry.pop_delta(), None);

        assert_eq!(registry.next(1, &[]), Some(ServiceDestination::Local));

        // sync for local service should has empty hops
        let sync = registry.sync_for(node1);
        assert_eq!(sync.0, vec![(1, Metric::local())]);
    }

    #[test]
    fn del_direct() {
        //let conn0: ConnId = ConnId::from_out(0, 0x0);
        let node0: NodeId = 0x0;

        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;

        let mut registry = Registry::new(node0);

        assert_eq!(registry.next(1, &[]), None);
        registry.apply_sync(conn1, node1, Metric::new(1, vec![1], BANDWIDTH_LIMIT), RegistrySync(vec![(1, Metric::new(1, vec![], BANDWIDTH_LIMIT))]));
        assert_eq!(registry.pop_delta(), Some(RegistryDelta::ServiceRemote(1, RegistryRemoteDestDelta::SetServicePath(conn1, 1, 12))));
        assert_eq!(registry.pop_delta(), None);

        assert_eq!(registry.next(1, &[]), Some(ServiceDestination::Remote(conn1, node1)));

        registry.del_direct(conn1);
        assert_eq!(registry.pop_delta(), Some(RegistryDelta::ServiceRemote(1, RegistryRemoteDestDelta::DelServicePath(conn1))));
        assert_eq!(registry.pop_delta(), None);
        assert_eq!(registry.next(1, &[]), None);
    }

    #[test]
    fn apply_sync() {
        let node0: NodeId = 0x0;
        let mut registry = Registry::new(node0);

        let conn1: ConnId = ConnId::from_out(0, 0x1);
        let node1: NodeId = 0x1;
        let _node2: NodeId = 0x2;
        let _node3: NodeId = 0x3;
        let node4: NodeId = 0x4;

        const SERVICE_1: u8 = 1;
        const SERVICE_2: u8 = 2;
        const SERVICE_3: u8 = 3;

        let sync = vec![(SERVICE_2, Metric::new(0, vec![], BANDWIDTH_LIMIT)), (SERVICE_3, Metric::new(0, vec![], BANDWIDTH_LIMIT))];
        registry.apply_sync(conn1, node1, Metric::new(1, vec![node1], BANDWIDTH_LIMIT), RegistrySync(sync));
        assert_eq!(
            registry.pop_delta(),
            Some(RegistryDelta::ServiceRemote(SERVICE_2, RegistryRemoteDestDelta::SetServicePath(conn1, node1, 11)))
        );
        assert_eq!(
            registry.pop_delta(),
            Some(RegistryDelta::ServiceRemote(SERVICE_3, RegistryRemoteDestDelta::SetServicePath(conn1, node1, 11)))
        );
        assert_eq!(registry.pop_delta(), None);

        assert_eq!(registry.next(SERVICE_1, &[]), None);
        assert_eq!(registry.next(SERVICE_2, &[]), Some(ServiceDestination::Remote(conn1, node1)));
        assert_eq!(registry.next(SERVICE_3, &[]), Some(ServiceDestination::Remote(conn1, node1)));

        let sync = vec![(SERVICE_3, Metric::new(0, vec![], BANDWIDTH_LIMIT))];
        registry.apply_sync(conn1, node1, Metric::new(1, vec![node1], BANDWIDTH_LIMIT), RegistrySync(sync));
        assert_eq!(registry.pop_delta(), Some(RegistryDelta::ServiceRemote(SERVICE_2, RegistryRemoteDestDelta::DelServicePath(conn1))));
        assert_eq!(registry.pop_delta(), None);

        assert_eq!(registry.next(SERVICE_1, &[]), None);
        assert_eq!(registry.next(SERVICE_2, &[]), None);
        assert_eq!(registry.next(SERVICE_3, &[]), Some(ServiceDestination::Remote(conn1, node1)));

        // check sync for other node
        let sync = registry.sync_for(node4);
        assert_eq!(sync, RegistrySync(vec![(SERVICE_3, Metric::new(1, vec![node1], BANDWIDTH_LIMIT))]));
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

        let sync = vec![(2, Metric::new(1, vec![node3, node2], BANDWIDTH_LIMIT))];
        registry.apply_sync(conn1, node1, Metric::new(1, vec![node1], BANDWIDTH_LIMIT), RegistrySync(sync));

        assert_eq!(registry.next(2, &[]), Some(ServiceDestination::Remote(conn1, node1)));
        assert_eq!(registry.sync_for(node1), RegistrySync(vec![]));
        assert_eq!(registry.sync_for(node2), RegistrySync(vec![]));
        assert_eq!(registry.sync_for(node3), RegistrySync(vec![]));
        assert_eq!(registry.sync_for(node4), RegistrySync(vec![(2, Metric::new(2, vec![node3, node2, node1], BANDWIDTH_LIMIT))]));
    }

    //TODO test multi connections with same node
}

use std::{fmt::Debug, hash::Hash};

use atm0s_sdn_identity::{NodeId, NodeIdType};

use crate::{RouteAction, RouterTable};

use self::{service::Service, table::ShadowTable};

mod service;
mod table;

#[derive(Debug, Clone)]
pub enum ShadowRouterDelta<Remote> {
    SetTable { layer: u8, index: u8, next: Remote },
    DelTable { layer: u8, index: u8 },
    SetServiceRemote { service: u8, next: Remote, dest: NodeId, score: u32 },
    DelServiceRemote { service: u8, next: Remote },
    SetServiceLocal { service: u8 },
    DelServiceLocal { service: u8 },
}

pub struct ShadowRouter<Remote> {
    node_id: NodeId,
    local_registries: [bool; 256],
    remote_registry: [Service<Remote>; 256],
    tables: [ShadowTable<Remote>; 4],
}

impl<Remote: Debug + Hash + Eq + Clone + Copy> ShadowRouter<Remote> {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            local_registries: [false; 256],
            remote_registry: std::array::from_fn(|_| Service::new()),
            tables: [ShadowTable::new(0), ShadowTable::new(1), ShadowTable::new(2), ShadowTable::new(3)],
        }
    }

    pub fn apply_delta(&mut self, delta: ShadowRouterDelta<Remote>) {
        match delta {
            ShadowRouterDelta::SetTable { layer, index, next: remote } => {
                self.tables[layer as usize].set(index, remote);
            }
            ShadowRouterDelta::DelTable { layer, index } => {
                self.tables[layer as usize].del(index);
            }
            ShadowRouterDelta::SetServiceRemote { service, next, dest, score } => {
                self.remote_registry[service as usize].set_conn(next, dest, score);
            }
            ShadowRouterDelta::DelServiceRemote { service, next } => {
                self.remote_registry[service as usize].del_conn(next);
            }
            ShadowRouterDelta::SetServiceLocal { service } => {
                self.local_registries[service as usize] = true;
            }
            ShadowRouterDelta::DelServiceLocal { service } => {
                self.local_registries[service as usize] = false;
            }
        }
    }

    fn next(&self, dest: NodeId) -> Option<Remote> {
        let eq_util_layer = self.node_id.eq_util_layer(&dest) as usize;
        debug_assert!(eq_util_layer <= 4);
        if eq_util_layer == 0 {
            None
        } else {
            self.tables[eq_util_layer - 1].next(dest)
        }
    }

    fn closest_for(&self, key: NodeId) -> Option<Remote> {
        for i in [3, 2, 1, 0] {
            let key_index = key.layer(i);
            if let Some((remote, _next_index, next_distance)) = self.tables[i as usize].closest_for(key_index) {
                let current_index = self.node_id.layer(i);
                let curent_distance = key_index ^ current_index;
                if curent_distance > next_distance {
                    return Some(remote);
                }
            } else {
                //if find nothing => that mean this layer is empty trying to find closest node in next layer
                continue;
            };
        }
        None
    }
}

impl<Remote: Debug + Hash + Eq + Clone + Copy> RouterTable<Remote> for ShadowRouter<Remote> {
    fn path_to_key(&self, key: NodeId) -> RouteAction<Remote> {
        match self.closest_for(key) {
            Some(remote) => RouteAction::Next(remote),
            None => RouteAction::Local,
        }
    }

    fn path_to_node(&self, dest: NodeId) -> RouteAction<Remote> {
        if dest == self.node_id {
            return RouteAction::Local;
        }
        match self.next(dest) {
            Some(remote) => RouteAction::Next(remote),
            None => RouteAction::Reject,
        }
    }

    fn path_to_service(&self, service_id: u8) -> RouteAction<Remote> {
        if self.local_registries[service_id as usize] {
            RouteAction::Local
        } else {
            self.remote_registry[service_id as usize].best_conn().map(RouteAction::Next).unwrap_or(RouteAction::Reject)
        }
    }

    fn path_to_services(&self, service_id: u8, level: crate::ServiceBroadcastLevel) -> RouteAction<Remote> {
        let local = self.local_registries[service_id as usize];
        if let Some(nexts) = self.remote_registry[service_id as usize].broadcast_dests(self.node_id, level) {
            RouteAction::Broadcast(local, nexts)
        } else if local {
            RouteAction::Local
        } else {
            RouteAction::Reject
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{RouteAction, RouterTable, ServiceBroadcastLevel};

    use super::{ShadowRouter, ShadowRouterDelta};

    #[test]
    fn should_route_to_next_service_local() {
        let mut router = ShadowRouter::<u64>::new(1);
        router.apply_delta(ShadowRouterDelta::SetServiceLocal { service: 1 });

        assert_eq!(router.path_to_service(0), RouteAction::Reject);
        assert_eq!(router.path_to_service(1), RouteAction::Local);
    }

    #[test]
    fn should_route_to_next_service_remote() {
        let mut router = ShadowRouter::<u64>::new(1);
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            next: 2,
            dest: 3,
            score: 4,
        });

        assert_eq!(router.path_to_service(0), RouteAction::Reject);
        assert_eq!(router.path_to_service(1), RouteAction::Next(2));
    }

    #[test]
    fn should_broadcast_to_next_service_local() {
        let mut router = ShadowRouter::<u64>::new(1);
        router.apply_delta(ShadowRouterDelta::SetServiceLocal { service: 1 });

        assert_eq!(router.path_to_services(1, ServiceBroadcastLevel::Global), RouteAction::Local);
    }

    #[test]
    fn should_broadcast_to_next_service_remote() {
        let mut router = ShadowRouter::<u64>::new(1);
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            next: 2,
            dest: 3,
            score: 4,
        });
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            next: 3,
            dest: 6,
            score: 2,
        });
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            next: 4,
            dest: 3,
            score: 1,
        });

        assert_eq!(
            router.path_to_services(1, ServiceBroadcastLevel::Global),
            RouteAction::Broadcast(false, vec![4, 3])
        );

        router.apply_delta(ShadowRouterDelta::SetServiceLocal { service: 1 });
        assert_eq!(
            router.path_to_services(1, ServiceBroadcastLevel::Global),
            RouteAction::Broadcast(true, vec![4, 3])
        );

        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            next: 4,
            dest: 5,
            score: 1,
        });
        assert_eq!(
            router.path_to_services(1, ServiceBroadcastLevel::Global),
            RouteAction::Broadcast(true, vec![4, 3, 2])
        );
    }
}

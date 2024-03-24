use std::{fmt::Debug, hash::Hash, sync::Arc};

use atm0s_sdn_identity::{NodeId, NodeIdType};

use crate::{RouteAction, RouterTable, ServiceBroadcastLevel};

use self::{service::Service, table::ShadowTable};

mod service;
mod table;

#[mockall::automock]
pub trait ShadowRouterHistory: Send + Sync {
    /// This methid will check if the broadcast message is already received or not
    /// If not received, it will cache the message and return true
    fn allready_received_broadcast(&self, from: Option<NodeId>, service: u8, seq: u16) -> bool;
}

#[derive(Debug, Clone)]
pub enum ShadowRouterDelta<Remote> {
    SetTable { layer: u8, index: u8, next: Remote },
    DelTable { layer: u8, index: u8 },
    SetServiceRemote { service: u8, conn: Remote, next: NodeId, dest: NodeId, score: u32 },
    DelServiceRemote { service: u8, conn: Remote },
    SetServiceLocal { service: u8 },
    DelServiceLocal { service: u8 },
}

pub struct ShadowRouter<Remote: Debug + Hash + Eq + Clone + Copy> {
    node_id: NodeId,
    local_registries: [bool; 256],
    remote_registry: [Service<Remote>; 256],
    tables: [ShadowTable<Remote>; 4],
    cached: Arc<dyn ShadowRouterHistory>,
}

impl<Remote: Debug + Hash + Eq + Clone + Copy> ShadowRouter<Remote> {
    pub fn new(node_id: NodeId, cached: Arc<dyn ShadowRouterHistory>) -> Self {
        Self {
            node_id,
            local_registries: [false; 256],
            remote_registry: std::array::from_fn(|_| Service::new()),
            tables: [ShadowTable::new(0), ShadowTable::new(1), ShadowTable::new(2), ShadowTable::new(3)],
            cached,
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
            ShadowRouterDelta::SetServiceRemote { service, conn, next, dest, score } => {
                self.remote_registry[service as usize].set_conn(conn, next, dest, score);
            }
            ShadowRouterDelta::DelServiceRemote { service, conn } => {
                self.remote_registry[service as usize].del_conn(conn);
            }
            ShadowRouterDelta::SetServiceLocal { service } => {
                self.local_registries[service as usize] = true;
            }
            ShadowRouterDelta::DelServiceLocal { service } => {
                self.local_registries[service as usize] = false;
            }
        }
    }
}

impl<Remote: Debug + Hash + Eq + Clone + Copy> RouterTable<Remote> for ShadowRouter<Remote> {
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
                let current_distance = key_index ^ current_index;
                if current_distance > next_distance {
                    return Some(remote);
                }
            } else {
                //if find nothing => that mean this layer is empty trying to find closest node in next layer
                continue;
            };
        }
        None
    }

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

    fn path_to_services(&self, service_id: u8, seq: u16, level: ServiceBroadcastLevel, source: Option<NodeId>, relay_from: Option<NodeId>) -> RouteAction<Remote> {
        if self.cached.allready_received_broadcast(source, service_id, seq) {
            return RouteAction::Reject;
        }
        let local = self.local_registries[service_id as usize];
        if let Some(nexts) = self.remote_registry[service_id as usize].broadcast_dests(self.node_id, level, relay_from) {
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
    use std::sync::Arc;

    use crate::{shadow::MockShadowRouterHistory, RouteAction, RouterTable, ServiceBroadcastLevel};

    use super::{ShadowRouter, ShadowRouterDelta};

    #[test]
    fn should_route_to_next_service_local() {
        let history = MockShadowRouterHistory::new();
        let mut router = ShadowRouter::<u64>::new(1, Arc::new(history));
        router.apply_delta(ShadowRouterDelta::SetServiceLocal { service: 1 });

        assert_eq!(router.path_to_service(0), RouteAction::Reject);
        assert_eq!(router.path_to_service(1), RouteAction::Local);
    }

    #[test]
    fn should_route_to_next_service_remote() {
        let history = MockShadowRouterHistory::new();
        let mut router = ShadowRouter::<u64>::new(1, Arc::new(history));
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            conn: 2,
            next: 2,
            dest: 3,
            score: 4,
        });

        assert_eq!(router.path_to_service(0), RouteAction::Reject);
        assert_eq!(router.path_to_service(1), RouteAction::Next(2));
    }

    #[test]
    fn should_broadcast_to_next_service_local() {
        let mut history = MockShadowRouterHistory::new();
        history.expect_allready_received_broadcast().return_const(true);
        let mut router = ShadowRouter::<u64>::new(1, Arc::new(history));
        router.apply_delta(ShadowRouterDelta::SetServiceLocal { service: 1 });

        assert_eq!(router.path_to_services(1, 1, ServiceBroadcastLevel::Global, None, None), RouteAction::Local);
    }

    #[test]
    fn should_broadcast_to_next_service_remote() {
        let mut history = MockShadowRouterHistory::new();
        history.expect_allready_received_broadcast().return_const(true);

        let mut router = ShadowRouter::<u64>::new(1, Arc::new(history));
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            conn: 2,
            next: 2,
            dest: 3,
            score: 4,
        });
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            conn: 3,
            next: 3,
            dest: 6,
            score: 2,
        });
        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            conn: 4,
            next: 4,
            dest: 3,
            score: 1,
        });

        assert_eq!(router.path_to_services(1, 1, ServiceBroadcastLevel::Global, None, None), RouteAction::Broadcast(false, vec![4, 3]));

        router.apply_delta(ShadowRouterDelta::SetServiceLocal { service: 1 });
        assert_eq!(router.path_to_services(1, 2, ServiceBroadcastLevel::Global, None, None), RouteAction::Broadcast(true, vec![4, 3]));

        router.apply_delta(ShadowRouterDelta::SetServiceRemote {
            service: 1,
            conn: 4,
            next: 4,
            dest: 5,
            score: 1,
        });
        assert_eq!(router.path_to_services(1, 3, ServiceBroadcastLevel::Global, None, Some(4)), RouteAction::Broadcast(true, vec![3, 2]));
    }

    #[test]
    fn reject_received_broadcast_message() {
        let mut history = MockShadowRouterHistory::new();
        history.expect_allready_received_broadcast().return_const(false);

        let mut router = ShadowRouter::<u64>::new(1, Arc::new(history));
        router.apply_delta(ShadowRouterDelta::SetServiceLocal { service: 100 });

        // should not broadcast if already received
        assert_eq!(router.path_to_services(100, 1, ServiceBroadcastLevel::Global, None, None), RouteAction::Reject);
    }
}

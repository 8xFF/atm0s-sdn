use atm0s_sdn_identity::{NodeId, NodeIdType};

use crate::{RouteAction, RouterTable};

use self::table::ShadowTable;

mod table;

#[derive(Debug, Clone)]
pub enum ShadowRouterDelta<Remote> {
    SetTable { layer: u8, index: u8, remote: Remote },
    DelTable { layer: u8, index: u8 },
    SetServiceRemote { service: u8, remote: Remote },
    DelServiceRemote { service: u8 },
    SetServiceLocal { service: u8 },
    DelServiceLocal { service: u8 },
}

pub struct ShadowRouter<Remote> {
    node_id: NodeId,
    local_registries: [bool; 256],
    remote_registry: [Option<Remote>; 256],
    tables: [ShadowTable<Remote>; 4],
}

impl<Remote: Clone + Copy> ShadowRouter<Remote> {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            local_registries: [false; 256],
            remote_registry: [None; 256],
            tables: [ShadowTable::new(0), ShadowTable::new(1), ShadowTable::new(2), ShadowTable::new(3)],
        }
    }

    pub fn apply_delta(&mut self, delta: ShadowRouterDelta<Remote>) {
        match delta {
            ShadowRouterDelta::SetTable { layer, index, remote } => {
                self.tables[layer as usize].set(index, remote);
            }
            ShadowRouterDelta::DelTable { layer, index } => {
                self.tables[layer as usize].del(index);
            }
            ShadowRouterDelta::SetServiceRemote { service, remote } => {
                self.remote_registry[service as usize] = Some(remote);
            }
            ShadowRouterDelta::DelServiceRemote { service } => {
                self.remote_registry[service as usize] = None;
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

impl<Remote: Clone + Copy> RouterTable<Remote> for ShadowRouter<Remote> {
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
            self.remote_registry[service_id as usize].clone().map(RouteAction::Next).unwrap_or(RouteAction::Reject)
        }
    }
}

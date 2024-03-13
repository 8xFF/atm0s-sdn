use std::{collections::HashMap, hash::Hash};

use atm0s_sdn_identity::NodeId;

use crate::ServiceBroadcastLevel;

#[derive(Debug, PartialEq, Eq)]
pub struct ServiceConn<Remote>(pub(crate) Remote, pub(crate) NodeId, pub(crate) u32);

impl<Remote: Eq + PartialEq> Ord for ServiceConn<Remote> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.2.cmp(&other.2)
    }
}

impl<Remote: Eq + PartialEq> PartialOrd for ServiceConn<Remote> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub struct Service<Remote> {
    dests: Vec<ServiceConn<Remote>>,
}

impl<Remote: Hash + Copy + Eq + PartialEq> Service<Remote> {
    pub fn new() -> Self {
        Self { dests: Vec::new() }
    }

    /// Add a new destination to the service, if Remote allready exists, it will be replaced
    pub fn set_conn(&mut self, conn: Remote, dest: NodeId, score: u32) {
        let index = self.dests.iter().position(|x| x.0 == conn);
        if let Some(index) = index {
            self.dests[index] = ServiceConn(conn, dest, score);
        } else {
            self.dests.push(ServiceConn(conn, dest, score));
        }
        self.dests.sort();
    }

    /// Remove a destination from the service
    pub fn del_conn(&mut self, conn: Remote) {
        self.dests.retain(|x| x.0 != conn);
    }

    pub fn best_conn(&self) -> Option<Remote> {
        self.dests.first().map(|x| x.0)
    }

    /// Get all unique destinations
    pub fn broadcast_dests(&self, node_id: NodeId, level: ServiceBroadcastLevel) -> Option<Vec<Remote>> {
        if self.dests.is_empty() {
            return None;
        }
        let mut dests = HashMap::new();
        for dest in &self.dests {
            if dests.contains_key(&dest.0) || !level.same_level(node_id, dest.2) {
                continue;
            }
            dests.insert(dest.0, ());
        }
        Some(dests.into_iter().map(|a| a.0).collect())
    }
}

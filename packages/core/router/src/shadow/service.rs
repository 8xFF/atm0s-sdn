use std::{collections::HashMap, fmt::Debug, hash::Hash};

use atm0s_sdn_identity::NodeId;

use crate::ServiceBroadcastLevel;

#[derive(Debug, PartialEq, Eq)]
pub struct ServiceConn<Conn, Remote> {
    pub(crate) conn: Conn,
    pub(crate) remote: Remote,
    pub(crate) next: NodeId,
    pub(crate) dest: NodeId,
    pub(crate) score: u32,
}

impl<Conn: Eq + PartialEq, Remote: Eq + PartialEq> Ord for ServiceConn<Conn, Remote> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.cmp(&other.score)
    }
}

impl<Conn: Eq + PartialEq, Remote: Eq + PartialEq> PartialOrd for ServiceConn<Conn, Remote> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.score.cmp(&other.score))
    }
}

pub struct Service<Conn, Remote> {
    dests: Vec<ServiceConn<Conn, Remote>>,
}

impl<Conn: Debug + Hash + Copy + Eq + PartialEq, Remote: Debug + Hash + Copy + Eq + PartialEq> Service<Conn, Remote> {
    pub fn new() -> Self {
        Self { dests: Vec::new() }
    }

    /// Add a new destination to the service, if Remote already exists, it will be replaced
    pub fn set_conn(&mut self, conn: Conn, remote: Remote, next: NodeId, dest: NodeId, score: u32) {
        let index = self.dests.iter().position(|x| x.conn == conn);
        if let Some(index) = index {
            self.dests[index] = ServiceConn { conn, remote, next, dest, score };
        } else {
            self.dests.push(ServiceConn { conn, remote, next, dest, score });
        }
        self.dests.sort();
    }

    /// Remove a destination from the service
    pub fn del_conn(&mut self, conn: Conn) {
        self.dests.retain(|x| x.conn != conn);
    }

    pub fn best_conn(&self) -> Option<Remote> {
        self.dests.first().map(|x| x.remote)
    }

    /// Get all unique destinations
    /// If relay_from is Some, it will not return the relay_from node connection
    pub fn broadcast_dests(&self, node_id: NodeId, level: ServiceBroadcastLevel, relay_from: Option<NodeId>) -> Option<Vec<Remote>> {
        if self.dests.is_empty() {
            return None;
        }
        let mut remotes = vec![];
        let mut dests = HashMap::new();
        for dest in &self.dests {
            if dests.contains_key(&dest.dest) || !level.same_level(node_id, dest.dest) {
                continue;
            }
            if let Some(relay_from) = &relay_from {
                if dest.next == *relay_from {
                    continue;
                }
            }
            dests.insert(dest.dest, ());
            remotes.push(dest.remote);
        }
        Some(remotes)
    }
}

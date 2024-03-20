use atm0s_sdn_identity::{NodeId, NodeIdType};
pub mod core;
pub mod shadow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceBroadcastLevel {
    Global,
    Geo1,
    Geo2,
    Group,
}

impl ServiceBroadcastLevel {
    pub fn same_level(&self, node1: NodeId, node2: NodeId) -> bool {
        match self {
            ServiceBroadcastLevel::Global => true,
            ServiceBroadcastLevel::Geo1 => node1.geo1() == node2.geo1(),
            ServiceBroadcastLevel::Geo2 => node1.geo1() == node2.geo1() && node1.geo2() == node2.geo2(),
            ServiceBroadcastLevel::Group => node1.geo1() == node2.geo1() && node1.geo2() == node2.geo2() && node1.group() == node2.group(),
        }
    }
}

impl Into<u8> for ServiceBroadcastLevel {
    fn into(self) -> u8 {
        match self {
            ServiceBroadcastLevel::Global => 0,
            ServiceBroadcastLevel::Geo1 => 1,
            ServiceBroadcastLevel::Geo2 => 2,
            ServiceBroadcastLevel::Group => 3,
        }
    }
}

impl From<u8> for ServiceBroadcastLevel {
    fn from(val: u8) -> Self {
        match val {
            0 => ServiceBroadcastLevel::Global,
            1 => ServiceBroadcastLevel::Geo1,
            2 => ServiceBroadcastLevel::Geo2,
            _ => ServiceBroadcastLevel::Group,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteRule {
    Direct,
    ToNode(NodeId),
    ToService(u8),
    ToServices(u8, ServiceBroadcastLevel),
    ToKey(NodeId),
}

/// Determine the destination of an action/message
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteAction<Remote> {
    /// Reject the message
    Reject,
    /// Will be processed locally
    Local,
    /// Will be forward to the given connection
    Next(Remote),
    /// Will be forward to the given connection, first is local or not, next is the list of remote dests
    Broadcast(bool, Vec<Remote>),
}

impl<Remote> RouteAction<Remote> {
    pub fn is_local(&self) -> bool {
        matches!(self, RouteAction::Local)
    }

    pub fn is_reject(&self) -> bool {
        matches!(self, RouteAction::Reject)
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, RouteAction::Next(_))
    }
}

pub trait RouterTable<Remote> {
    /// Determine the next action for the given destination node
    fn path_to_node(&self, dest: NodeId) -> RouteAction<Remote>;
    /// Determine the next action for the given key
    fn path_to_key(&self, key: NodeId) -> RouteAction<Remote>;
    /// Determine the next action for the given service
    fn path_to_service(&self, service_id: u8) -> RouteAction<Remote>;
    /// Determine the next action if we need broadcast to all node running a service.
    /// If relay_from is set, it should not sending back for avoiding loop
    fn path_to_services(&self, service_id: u8, level: ServiceBroadcastLevel, relay_from: Option<NodeId>) -> RouteAction<Remote>;
    /// Determine next action for incoming messages
    /// given the route rule and service id
    fn derive_action(&self, route: &RouteRule, relay_from: Option<NodeId>) -> RouteAction<Remote> {
        match route {
            RouteRule::Direct => RouteAction::Local,
            RouteRule::ToNode(dest) => self.path_to_node(*dest),
            RouteRule::ToKey(key) => self.path_to_key(*key),
            RouteRule::ToService(service) => self.path_to_service(*service),
            RouteRule::ToServices(service, level) => self.path_to_services(*service, *level, relay_from),
        }
    }
}

#[cfg(test)]
mod tests {
    use atm0s_sdn_identity::ConnId;
    type RouteAction = super::RouteAction<ConnId>;

    #[test]
    fn test_is_local() {
        let local = RouteAction::Local;
        let remote = RouteAction::Next(ConnId::from_in(1, 1));
        let reject = RouteAction::Reject;

        assert!(local.is_local());
        assert!(!remote.is_local());
        assert!(!reject.is_local());
    }

    #[test]
    fn test_is_reject() {
        let local = RouteAction::Local;
        let remote = RouteAction::Next(ConnId::from_in(1, 1));
        let reject = RouteAction::Reject;

        assert!(!local.is_reject());
        assert!(!remote.is_reject());
        assert!(reject.is_reject());
    }

    #[test]
    fn test_is_remote() {
        let local = RouteAction::Local;
        let remote = RouteAction::Next(ConnId::from_in(1, 1));
        let reject = RouteAction::Reject;

        assert!(!local.is_remote());
        assert!(remote.is_remote());
        assert!(!reject.is_remote());
    }
}

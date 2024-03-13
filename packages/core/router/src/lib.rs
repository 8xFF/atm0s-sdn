use atm0s_sdn_identity::NodeId;
pub mod core;
pub mod shadow;

/// ServiceMeta is using for determine which node will be routed, example node with lowest price or lowest latency, which for future use
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceMeta {
    Closest,
    Broadcast,
}

impl ServiceMeta {
    pub fn to_be_bytes(&self) -> [u8; 4] {
        match self {
            ServiceMeta::Closest => [0, 0, 0, 0],
            ServiceMeta::Broadcast => [1, 1, 1, 1],
        }
    }
}

impl From<[u8; 4]> for ServiceMeta {
    fn from(buf: [u8; 4]) -> Self {
        match buf[0] {
            0 => ServiceMeta::Closest,
            _ => ServiceMeta::Broadcast,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteRule {
    Direct,
    ToNode(NodeId),
    ToService(ServiceMeta),
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
    fn path_to_service(&self, service_id: u8, meta: ServiceMeta) -> RouteAction<Remote>;
    /// Determine next action for incoming messages
    /// given the route rule and service id
    fn derive_action(&self, route: &RouteRule, service_id: u8) -> RouteAction<Remote> {
        match route {
            RouteRule::Direct => RouteAction::Local,
            RouteRule::ToNode(dest) => self.path_to_node(*dest),
            RouteRule::ToKey(key) => self.path_to_key(*key),
            RouteRule::ToService(meta) => self.path_to_service(service_id, *meta),
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

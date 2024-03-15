use atm0s_sdn_identity::NodeId;

enum State {
    Connecting { at_ms: u64 },
    Connected { at_ms: u64 },
    Disconnecting { at_ms: u64 },
    Disconnected,
}

pub struct NeighbourConnection {
    from: NodeId,
    to: NodeId,
    session: u64,
    state: State,
}

impl NeighbourConnection {
    pub fn new_outgoing(from: NodeId, to: NodeId, session: u64, now_ms: u64) -> Self {
        Self {
            from,
            to,
            session,
            state: State::Connecting { at_ms: now_ms },
        }
    }

    pub fn new_incoming(from: NodeId, to: NodeId, session: u64, now_ms: u64) -> Self {
        Self {
            from,
            to,
            session,
            state: State::Connected { at_ms: now_ms },
        }
    }
}

use bluesea_identity::{NodeAddr, NodeId};

pub enum EntryState {
    Connecting { distance: NodeId, addr: NodeAddr, started_at: u64 },
    Connected { distance: NodeId, addr: NodeAddr, started_at: u64 },
    Empty,
}

pub struct Entry {
    state: EntryState,
}

impl Entry {
    pub(crate) fn new() -> Self {
        Self { state: EntryState::Empty }
    }

    pub fn is_empty(&self) -> bool {
        matches!(&self.state, &EntryState::Empty)
    }

    pub fn is_connected(&self) -> bool {
        matches!(&self.state, &EntryState::Connected { .. })
    }

    pub fn is_connecting(&self) -> bool {
        matches!(&self.state, &EntryState::Connecting { .. })
    }

    pub fn state(&self) -> &EntryState {
        &self.state
    }

    pub fn switch_state(&mut self, state: EntryState) {
        self.state = state;
    }
}

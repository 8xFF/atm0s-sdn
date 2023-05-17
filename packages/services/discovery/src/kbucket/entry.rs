use bluesea_identity::{PeerAddr, PeerId};

pub enum EntryState {
    Connecting {
        distance: PeerId,
        addr: PeerAddr,
        started_at: u64,
    },
    Connected {
        distance: PeerId,
        addr: PeerAddr,
        started_at: u64,
    },
    Empty,
}

pub struct Entry {
    state: EntryState,
}

impl Entry {
    pub(crate) fn new() -> Self {
        Self {
            state: EntryState::Empty,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(&self.state, &EntryState::Empty)
    }

    pub fn state(&self) -> &EntryState {
        &self.state
    }

    pub fn switch_state(&mut self, state: EntryState) {
        self.state = state;
    }
}

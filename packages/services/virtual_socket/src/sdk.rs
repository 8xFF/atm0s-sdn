use std::sync::Arc;

use parking_lot::RwLock;

use crate::state::{connector::VirtualSocketConnector, listener::VirtualSocketListener, State};

#[derive(Clone)]
pub struct VirtualSocketSdk {
    state: Arc<RwLock<State>>,
}

impl VirtualSocketSdk {
    pub fn new(state: Arc<RwLock<State>>) -> Self {
        Self { state }
    }

    pub fn connector(&self) -> VirtualSocketConnector {
        VirtualSocketConnector { state: self.state.clone() }
    }

    pub fn listen(&self, id: &str) -> VirtualSocketListener {
        log::info!("[VirtualSocketSdk] listen on: {}", id);
        let rx = self.state.write().new_listener(id);
        VirtualSocketListener { rx, state: self.state.clone() }
    }
}

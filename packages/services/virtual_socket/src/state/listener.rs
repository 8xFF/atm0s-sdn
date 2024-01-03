use std::sync::Arc;

use parking_lot::RwLock;

use super::{
    socket::{VirtualSocket, VirtualSocketBuilder},
    State,
};

pub struct VirtualSocketListener {
    pub(crate) rx: async_std::channel::Receiver<VirtualSocketBuilder>,
    pub(crate) state: Arc<RwLock<State>>,
}

impl VirtualSocketListener {
    pub async fn recv(&mut self) -> Option<VirtualSocket> {
        let builder = self.rx.recv().await.ok()?;
        Some(builder.build(self.state.clone()))
    }
}

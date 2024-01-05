pub(crate) const VIRTUAL_SOCKET_SERVICE_ID: u8 = 6;

mod behavior;
mod handler;
mod msg;
mod sdk;
pub(crate) mod state;

pub use behavior::VirtualSocketBehavior;
pub use sdk::VirtualSocketSdk;
pub use state::{socket::VirtualSocket, stream::VirtualStream, VirtualSocketConnectResult};

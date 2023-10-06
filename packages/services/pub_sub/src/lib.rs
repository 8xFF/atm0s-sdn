pub static PUBSUB_SERVICE_ID: u8 = 5;
pub(crate) static PUBSUB_CHANNEL_RESYNC_MS: u64 = 5000;
pub(crate) static PUBSUB_CHANNEL_TIMEOUT_MS: u64 = 20000;

mod behaviour;
mod handler;
mod msg;
pub(crate) mod relay;
pub(crate) mod sdk;

pub use behaviour::PubSubServiceBehaviour;
pub use sdk::{consumer::Consumer, publisher::Publisher, PubsubSdk};

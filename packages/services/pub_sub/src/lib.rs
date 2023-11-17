pub static PUBSUB_SERVICE_ID: u8 = 5;
pub(crate) static PUBSUB_CHANNEL_RESYNC_MS: u64 = 5000;
pub(crate) static PUBSUB_CHANNEL_TIMEOUT_MS: u64 = 20000;

mod behaviour;
mod handler;
mod msg;
mod relay;
mod sdk;

pub use behaviour::PubsubServiceBehaviour;
pub use msg::{PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
pub use relay::{feedback::Feedback, feedback::FeedbackType, feedback::NumberInfo, ChannelIdentify, ChannelUuid, LocalPubId, LocalSubId};
pub use sdk::{consumer::Consumer, consumer_raw::ConsumerRaw, consumer_single::ConsumerSingle, publisher::Publisher, publisher_raw::PublisherRaw, PubsubSdk};

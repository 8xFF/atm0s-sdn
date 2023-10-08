pub static PUBSUB_SERVICE_ID: u8 = 5;
pub(crate) static PUBSUB_CHANNEL_RESYNC_MS: u64 = 5000;
pub(crate) static PUBSUB_CHANNEL_TIMEOUT_MS: u64 = 20000;

mod behaviour;
mod handler;
mod msg;
pub(crate) mod relay;
pub(crate) mod sdk;

pub use behaviour::PubsubServiceBehaviour;
pub use msg::{PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
pub use relay::ChannelIdentify;
pub use sdk::{consumer::Consumer, publisher::Publisher, PubsubSdk};

#[cfg(test)]
mod tests {
    use async_std::prelude::FutureExt;
    use bluesea_router::ForceLocalRouter;
    use bytes::Bytes;
    use network::mock::MockTransport;
    use network::{
        convert_enum,
        plane::{NetworkPlane, NetworkPlaneConfig},
    };
    use std::{sync::Arc, time::Duration, vec};
    use utils::{option_handle::OptionUtils, SystemTimer};

    use crate::msg::{PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
    use crate::PubsubServiceBehaviour;

    #[derive(convert_enum::From, convert_enum::TryInto, PartialEq, Debug)]
    enum ImplNetworkMsg {
        Pubsub(PubsubRemoteEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplBehaviorEvent {
        Pubsub(PubsubServiceBehaviourEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplHandlerEvent {
        Pubsub(PubsubServiceHandlerEvent),
    }

    /// Testing local storage
    #[async_std::test]
    async fn local_node() {
        let (mock, _faker, _output) = MockTransport::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let node_id = 0;
        let (behavior, sdk) = PubsubServiceBehaviour::new(node_id);

        let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent>::new(NetworkPlaneConfig {
            local_node_id: 0,
            tick_ms: 100,
            behavior: vec![Box::new(behavior)],
            transport,
            timer,
            router: Arc::new(ForceLocalRouter()),
        });

        let join = async_std::task::spawn(async move {
            plane.started();
            while let Ok(_) = plane.recv().await {}
            plane.stopped();
        });

        async_std::task::sleep(Duration::from_millis(1000)).await;

        let producer = sdk.create_publisher(1111);
        let consumer = sdk.create_consumer(producer.identify(), Some(10));

        let data = Bytes::from(vec![1, 2, 3, 4]);
        producer.send(data.clone());
        let got_value = consumer.recv().timeout(Duration::from_secs(1)).await.expect("Should get success").expect("Should some");
        assert_eq!(&got_value, &data);

        join.cancel().await.print_none("Should cancel join");
    }
}

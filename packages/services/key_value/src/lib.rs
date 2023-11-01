pub static KEY_VALUE_SERVICE_ID: u8 = 4;
pub type KeyId = u64;
pub type SubKeyId = u64;
pub type ReqId = u64;
pub type KeyVersion = u64;
pub type KeySource = NodeId;
pub type ValueType = Vec<u8>;

mod behavior;
mod handler;
mod msg;
mod storage;

pub use behavior::KeyValueBehavior;
pub use behavior::KeyValueSdk;
use bluesea_identity::NodeId;
pub use msg::{KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueMsg};

#[cfg(test)]
mod tests {
    // use std::{sync::Arc, time::Duration, vec};

    // use bluesea_router::ForceLocalRouter;
    // use network::mock::MockTransport;
    // use network::{
    //     convert_enum,
    //     plane::{NetworkPlane, NetworkPlaneConfig},
    // };
    // use utils::{option_handle::OptionUtils, SystemTimer};

    // use crate::{KeyValueBehavior, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueMsg};

    // #[derive(convert_enum::From, convert_enum::TryInto, PartialEq, Debug)]
    // enum ImplNetworkMsg {
    //     KeyValue(KeyValueMsg),
    // }

    // #[derive(convert_enum::From, convert_enum::TryInto)]
    // enum ImplBehaviorEvent {
    //     KeyValue(KeyValueBehaviorEvent),
    // }

    // #[derive(convert_enum::From, convert_enum::TryInto)]
    // enum ImplHandlerEvent {
    //     KeyValue(KeyValueHandlerEvent),
    // }

    // /// Testing local storage
    // #[async_std::test]
    // async fn local_node() {
    //     let (mock, _faker, _output) = MockTransport::new();
    //     let transport = Box::new(mock);
    //     let timer = Arc::new(SystemTimer());

    //     let (behavior, sdk) = KeyValueBehavior::new(0, timer.clone(), 1000);

    //     let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent>::new(NetworkPlaneConfig {
    //         local_node_id: 0,
    //         tick_ms: 100,
    //         behavior: vec![Box::new(behavior)],
    //         transport,
    //         timer,
    //         router: Arc::new(ForceLocalRouter()),
    //     });

    //     let join = async_std::task::spawn(async move {
    //         plane.started();
    //         while let Ok(_) = plane.recv().await {}
    //         plane.stopped();
    //     });

    //     async_std::task::sleep(Duration::from_millis(1000)).await;
    //     sdk.set(111, vec![111], None);
    //     let saved_value = sdk.get(111, 1000).await.expect("Should get success").expect("Should some");
    //     assert_eq!(saved_value.0, vec![111]);

    //     join.cancel().await.print_none("Should cancel join");
    // }

    // /// Testing local storage
    // #[async_std::test]
    // async fn local_node_hashmap() {
    //     let (mock, _faker, _output) = MockTransport::new();
    //     let transport = Box::new(mock);
    //     let timer = Arc::new(SystemTimer());

    //     let (behavior, sdk) = KeyValueBehavior::new(0, timer.clone(), 1000);

    //     let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent>::new(NetworkPlaneConfig {
    //         local_node_id: 0,
    //         tick_ms: 100,
    //         behavior: vec![Box::new(behavior)],
    //         transport,
    //         timer,
    //         router: Arc::new(ForceLocalRouter()),
    //     });

    //     let join = async_std::task::spawn(async move {
    //         plane.started();
    //         while let Ok(_) = plane.recv().await {}
    //         plane.stopped();
    //     });

    //     async_std::task::sleep(Duration::from_millis(1000)).await;
    //     sdk.hset(111, 222, vec![111], None);
    //     let saved_value = sdk.hget(111, 1000).await.expect("Should get success").expect("Should some");
    //     assert_eq!(saved_value.into_iter().map(|(sub, val, _, src)| (sub, val, src)).collect::<Vec<_>>(), vec![(222, vec![111], 0)]);

    //     join.cancel().await.print_none("Should cancel join");
    // }
}

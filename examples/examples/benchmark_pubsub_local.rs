use std::{sync::Arc, time::Duration};

use atm0s_sdn::convert_enum;
use atm0s_sdn::SharedRouter;
use atm0s_sdn::{KeyValueBehavior, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent};
use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use atm0s_sdn::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
use atm0s_sdn::{NetworkPlane, NetworkPlaneConfig};
use atm0s_sdn::{NodeAddr, NodeAddrBuilder, NodeId, UdpTransport};
use atm0s_sdn::{PubsubSdk, PubsubServiceBehaviour, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
use atm0s_sdn::{SystemTimer, Timer};
use bytes::Bytes;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplBehaviorEvent {
    Pubsub(PubsubServiceBehaviourEvent),
    KeyValue(KeyValueBehaviorEvent),
    RouterSync(LayersSpreadRouterSyncBehaviorEvent),
    Manual(ManualBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplHandlerEvent {
    Pubsub(PubsubServiceHandlerEvent),
    KeyValue(KeyValueHandlerEvent),
    RouterSync(LayersSpreadRouterSyncHandlerEvent),
    Manual(ManualHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplSdkEvent {
    KeyValue(KeyValueSdkEvent),
}

async fn run_node(node_id: NodeId, seeds: Vec<NodeAddr>) -> (PubsubSdk, NodeAddr) {
    log::info!("Run node {} connect to {:?}", node_id, seeds);
    let secure = Arc::new(atm0s_sdn::StaticKeySecure::new("secure-token"));
    let mut node_addr_builder = NodeAddrBuilder::new(node_id);
    let socket = UdpTransport::prepare(0, &mut node_addr_builder).await;
    let transport = Box::new(UdpTransport::new(node_addr_builder.addr(), socket, secure));
    let timer = Arc::new(SystemTimer());

    let router = SharedRouter::new(node_id);
    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id,
        node_addr: node_addr_builder.addr(),
        seeds,
        local_tags: vec![],
        connect_tags: vec![],
    });

    let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());
    let kv_behaviour = KeyValueBehavior::new(node_id, 3000, None);
    let (pubsub_behavior, pubsub_sdk) = PubsubServiceBehaviour::new(node_id, timer.clone());

    let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent, ImplSdkEvent>::new(NetworkPlaneConfig {
        node_id,
        tick_ms: 1000,
        behaviors: vec![Box::new(pubsub_behavior), Box::new(kv_behaviour), Box::new(router_sync_behaviour), Box::new(manual)],
        transport,
        timer,
        router: Arc::new(router.clone()),
    });

    async_std::task::spawn(async move {
        plane.started();
        while let Ok(_) = plane.recv().await {}
        plane.stopped();
    });

    (pubsub_sdk, node_addr_builder.addr())
}

#[async_std::main]
async fn main() {
    env_logger::init();
    let node_id1 = 1;

    let (pubsub_sdk, _node_addr) = run_node(node_id1, vec![]).await;

    let ping_producer = pubsub_sdk.create_publisher(1);
    let pong_producer = pubsub_sdk.create_publisher(2);

    let pong_consumer = pubsub_sdk.create_consumer_single(pong_producer.identify(), None);
    let ping_consumer = pubsub_sdk.create_consumer_single(ping_producer.identify(), None);

    async_std::task::sleep(Duration::from_secs(1)).await;

    async_std::task::spawn(async move {
        while let Some((_sub_id, _source, _channel, msg)) = ping_consumer.recv().await {
            pong_producer.send(msg);
        }
    });

    let timer = SystemTimer();
    let data = Bytes::from(vec![0; 1000]);
    let mut pre_ts = timer.now_ms();
    let mut count = 0;

    //bootstrap
    for _ in 0..20 {
        ping_producer.send(data.clone());
    }

    while count < 1000000 {
        count += 1;
        if count % 100000 == 0 {
            let now_ms = timer.now_ms();
            log::info!("Sent {} msg -> speed {} pps", count, 100000 * 1000 / (now_ms - pre_ts));
            pre_ts = now_ms;
        }
        pong_consumer.recv().await.expect("Should received");
        ping_producer.send(data.clone());
    }
}

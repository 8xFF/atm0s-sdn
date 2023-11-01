use std::{sync::Arc, time::Duration};

use bluesea_identity::{NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use bytes::Bytes;
use key_value::{KeyValueBehavior, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueMsg};
use layers_spread_router::SharedRouter;
use layers_spread_router_sync::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, LayersSpreadRouterSyncMsg};
use manual_discovery::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, ManualMsg};
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use pub_sub::{ChannelSourceHashmapReal, PubsubRemoteEvent, PubsubSdk, PubsubServiceBehaviour, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
use utils::{SystemTimer, Timer};

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplNetworkMsg {
    Pubsub(PubsubRemoteEvent),
    KeyValue(KeyValueMsg),
    RouterSync(LayersSpreadRouterSyncMsg),
    Manual(ManualMsg),
}

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

async fn run_node(node_id: NodeId, neighbours: Vec<NodeAddr>) -> (PubsubSdk, NodeAddr) {
    log::info!("Run node {} connect to {:?}", node_id, neighbours);
    let node_addr = Arc::new(NodeAddrBuilder::default());
    node_addr.add_protocol(Protocol::P2p(node_id));
    let transport = Box::new(transport_udp::UdpTransport::new(node_id, 0, node_addr.clone()).await);
    let timer = Arc::new(SystemTimer());

    let router = SharedRouter::new(node_id);
    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id,
        neighbours,
        timer: timer.clone(),
    });

    let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());
    let (kv_behaviour, kv_sdk) = KeyValueBehavior::new(node_id, timer.clone(), 3000);
    let (pubsub_behavior, pubsub_sdk) = PubsubServiceBehaviour::new(node_id, Box::new(ChannelSourceHashmapReal::new(kv_sdk, node_id)), timer.clone());

    let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent>::new(NetworkPlaneConfig {
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

    (pubsub_sdk, node_addr.addr())
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

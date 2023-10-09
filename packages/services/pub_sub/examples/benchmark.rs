use std::{sync::Arc, time::Duration};

use bluesea_identity::{NodeAddr, NodeAddrBuilder, NodeId, Protocol};
use bytes::Bytes;
use layers_spread_router::SharedRouter;
use layers_spread_router_sync::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, LayersSpreadRouterSyncMsg};
use manual_discovery::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, ManualMsg};
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use pub_sub::{PubsubRemoteEvent, PubsubSdk, PubsubServiceBehaviour, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
use utils::{SystemTimer, Timer};

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplNetworkMsg {
    Pubsub(PubsubRemoteEvent),
    RouterSync(LayersSpreadRouterSyncMsg),
    Manual(ManualMsg),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplBehaviorEvent {
    Pubsub(PubsubServiceBehaviourEvent),
    RouterSync(LayersSpreadRouterSyncBehaviorEvent),
    Manual(ManualBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplHandlerEvent {
    Pubsub(PubsubServiceHandlerEvent),
    RouterSync(LayersSpreadRouterSyncHandlerEvent),
    Manual(ManualHandlerEvent),
}

async fn run_node(node_id: NodeId, neighbours: Vec<NodeAddr>) -> (PubsubSdk<ImplBehaviorEvent, ImplHandlerEvent>, NodeAddr) {
    log::info!("Run node {} connect to {:?}", node_id, neighbours);
    let node_addr = Arc::new(NodeAddrBuilder::default());
    node_addr.add_protocol(Protocol::P2p(node_id));
    let transport = Box::new(transport_udp::UdpTransport::new(node_id, 0, node_addr.clone()).await);
    let timer = Arc::new(SystemTimer());

    let router = SharedRouter::new(node_id);
    let manual = ManualBehavior::new(ManualBehaviorConf { neighbours, timer: timer.clone() });

    let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());
    let (pubsub_behavior, pubsub_sdk) = PubsubServiceBehaviour::new(node_id);

    let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent>::new(NetworkPlaneConfig {
        local_node_id: node_id,
        tick_ms: 1000,
        behavior: vec![Box::new(pubsub_behavior), Box::new(router_sync_behaviour), Box::new(manual)],
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
    let node_id2 = 2;

    let (pubsub_sdk1, node_addr1) = run_node(node_id1, vec![]).await;
    let (pubsub_sdk2, _node_addr2) = run_node(node_id2, vec![node_addr1]).await;

    let ping_producer = pubsub_sdk1.create_publisher(1);
    let pong_producer = pubsub_sdk2.create_publisher(2);

    let pong_consumer = pubsub_sdk1.create_consumer(pong_producer.identify(), None);
    let ping_consumer = pubsub_sdk2.create_consumer(ping_producer.identify(), None);

    async_std::task::sleep(Duration::from_secs(5)).await;

    async_std::task::spawn(async move {
        while let Some(msg) = ping_consumer.recv().await {
            pong_producer.send(msg);
        }
    });

    let timer = SystemTimer();
    let data = Bytes::from(vec![0; 1000]);
    let mut pre_ts = timer.now_ms();
    let mut count = 0;

    //bootstrap
    for _ in 0..5 {
        ping_producer.send(data.clone());
    }

    while count < 1000000 {
        count += 1;
        if count % 10000 == 0 {
            let now_ms = timer.now_ms();
            log::info!("Sent {} msg -> speed {} pps", count, 10000 * 1000 / (now_ms - pre_ts));
            pre_ts = now_ms;
        }
        pong_consumer.recv().await.expect("Should received");
        ping_producer.send(data.clone());
    }
}
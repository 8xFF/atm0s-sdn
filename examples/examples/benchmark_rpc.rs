use std::time::Instant;
use std::{sync::Arc, time::Duration};

use atm0s_sdn::convert_enum;
use atm0s_sdn::KeyValueBehaviorEvent;
use atm0s_sdn::KeyValueHandlerEvent;
use atm0s_sdn::KeyValueSdkEvent;
use atm0s_sdn::ManualBehavior;
use atm0s_sdn::ManualBehaviorConf;
use atm0s_sdn::ManualBehaviorEvent;
use atm0s_sdn::ManualHandlerEvent;
use atm0s_sdn::RouteRule;
use atm0s_sdn::RpcBox;
use atm0s_sdn::SharedRouter;
use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use atm0s_sdn::{NetworkPlane, NetworkPlaneConfig};
use atm0s_sdn::{NodeAddr, NodeAddrBuilder, NodeId, UdpTransport};
use atm0s_sdn::{SystemTimer, Timer};

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplBehaviorEvent {
    RouterSync(LayersSpreadRouterSyncBehaviorEvent),
    KeyValue(KeyValueBehaviorEvent),
    Manual(ManualBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplHandlerEvent {
    RouterSync(LayersSpreadRouterSyncHandlerEvent),
    KeyValue(KeyValueHandlerEvent),
    Manual(ManualHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplSdkEvent {
    KeyValue(KeyValueSdkEvent),
}

async fn run_node(node_id: NodeId, seeds: Vec<NodeAddr>) -> (RpcBox, NodeAddr) {
    log::info!("Run node {}", node_id);
    let secure = Arc::new(atm0s_sdn::StaticKeySecure::new("secure-token"));
    let mut node_addr_builder = NodeAddrBuilder::new(node_id);
    let socket = UdpTransport::prepare(0, &mut node_addr_builder).await;
    let transport = Box::new(UdpTransport::new(node_addr_builder.addr(), socket, secure));
    let timer = Arc::new(SystemTimer());

    let router = SharedRouter::new(node_id);
    let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());
    let mut rpc_box = RpcBox::new(node_id, 100, timer.clone());
    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id,
        node_addr: node_addr_builder.addr(),
        seeds,
        local_tags: vec![],
        connect_tags: vec![],
    });

    let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent, ImplSdkEvent>::new(NetworkPlaneConfig {
        node_id,
        tick_ms: 100,
        behaviors: vec![Box::new(rpc_box.behaviour()), Box::new(router_sync_behaviour), Box::new(manual)],
        transport,
        timer,
        router: Arc::new(router.clone()),
    });

    async_std::task::spawn(async move {
        plane.started();
        while let Ok(_) = plane.recv().await {}
        plane.stopped();
    });

    (rpc_box, node_addr_builder.addr())
}

#[async_std::main]
async fn main() {
    env_logger::init();
    let node_id1 = 1;
    let node_id2 = 2;

    let (mut rpc_box1, node_addr1) = run_node(node_id1, vec![]).await;
    let (mut rpc_box2, _node_addr2) = run_node(node_id2, vec![node_addr1]).await;

    async_std::task::sleep(Duration::from_secs(5)).await;

    let rpc_emitter = rpc_box1.emitter();
    let rpc_emitter2 = rpc_box2.emitter();
    async_std::task::spawn(async move {
        while let Some(rpc_msg) = rpc_box2.recv().await {
            rpc_emitter2.answer_for::<Vec<u8>>(rpc_msg, Ok(vec![1, 2, 3]));
        }
    });

    let timer = SystemTimer();
    let mut pre_ts = timer.now_ms();
    let mut count = 0;
    let mut max_duration = 0;

    while count < 100000 {
        count += 1;
        if count % 10000 == 0 {
            let now_ms = timer.now_ms();
            println!("Sent {} msg -> speed {} pps, max_duration {}", count, 10000 * 1000 / (now_ms - pre_ts), max_duration);
            pre_ts = now_ms;
            max_duration = 0;
        }
        let started = Instant::now();
        rpc_emitter
            .request::<Vec<u8>, Vec<u8>>(100, RouteRule::ToNode(node_id2), "echo", vec![1, 2, 3], 1000)
            .await
            .expect("Should send rpc");

        let duration = started.elapsed().as_millis();
        if max_duration < duration {
            max_duration = duration;
        }
    }
}

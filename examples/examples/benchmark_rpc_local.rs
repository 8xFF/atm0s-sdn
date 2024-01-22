use std::{sync::Arc, time::Duration};

use atm0s_sdn::convert_enum;
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
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplHandlerEvent {
    RouterSync(LayersSpreadRouterSyncHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum ImplSdkEvent {}

async fn run_node(node_id: NodeId) -> (RpcBox, NodeAddr) {
    log::info!("Run node {}", node_id);
    let secure = Arc::new(atm0s_sdn::StaticKeySecure::new("secure-token"));
    let mut node_addr_builder = NodeAddrBuilder::new(node_id);
    let socket = UdpTransport::prepare(0, &mut node_addr_builder).await;
    let transport = Box::new(UdpTransport::new(node_addr_builder.addr(), socket, secure));
    let timer = Arc::new(SystemTimer());

    let router = SharedRouter::new(node_id);
    let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());
    let mut rpc_box = RpcBox::new(node_id, 100, timer.clone());

    let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent, ImplSdkEvent>::new(NetworkPlaneConfig {
        node_id,
        tick_ms: 1000,
        behaviors: vec![Box::new(rpc_box.behaviour()), Box::new(router_sync_behaviour)],
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

    let (mut rpc_box, _node_addr) = run_node(node_id1).await;

    let rpc_emitter = rpc_box.emitter();

    async_std::task::sleep(Duration::from_secs(1)).await;

    let rpc_emitter2 = rpc_box.emitter();
    async_std::task::spawn(async move {
        while let Some(rpc_msg) = rpc_box.recv().await {
            rpc_emitter2.answer_for::<Vec<u8>>(rpc_msg, Ok(vec![1, 2, 3]));
        }
    });

    let timer = SystemTimer();
    let mut pre_ts = timer.now_ms();
    let mut count = 0;

    while count < 100000 {
        count += 1;
        if count % 10000 == 0 {
            let now_ms = timer.now_ms();
            println!("Sent {} msg -> speed {} pps", count, 10000 * 1000 / (now_ms - pre_ts));
            pre_ts = now_ms;
        }
        rpc_emitter
            .request::<Vec<u8>, Vec<u8>>(100, RouteRule::ToService(0), "echo", vec![1, 2, 3], 10000)
            .await
            .expect("Should request rpc");
    }
}

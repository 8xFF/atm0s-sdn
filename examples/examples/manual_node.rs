use bluesea_identity::{NodeAddr, NodeAddrBuilder, Protocol};
use clap::Parser;
use key_value::{KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent};
use layers_spread_router::SharedRouter;
use layers_spread_router_sync::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use manual_discovery::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use redis_server::RedisServer;
use std::net::SocketAddr;
use std::sync::Arc;
use tun_tap::{TunTapBehaviorEvent, TunTapHandlerEvent};
use utils::SystemTimer;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncBehaviorEvent),
    TunTap(TunTapBehaviorEvent),
    KeyValue(KeyValueBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncHandlerEvent),
    TunTap(TunTapHandlerEvent),
    KeyValue(KeyValueHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeSdkEvent {
    KeyValue(KeyValueSdkEvent),
}

/// Node with manual network builder
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Current Node ID
    #[arg(env, long)]
    node_id: u32,

    /// Neighbors
    #[arg(env, long)]
    neighbours: Vec<NodeAddr>,

    /// Enable tun-tap
    #[arg(env, long)]
    tun_tap: bool,

    /// Simple Redis KeyValue server
    #[arg(env, long)]
    redis_addr: Option<SocketAddr>,
}

#[async_std::main]
async fn main() {
    env_logger::builder().format_timestamp_millis().init();
    let args: Args = Args::parse();
    let node_addr_builder = Arc::new(NodeAddrBuilder::default());
    node_addr_builder.add_protocol(Protocol::P2p(args.node_id));
    let transport = transport_udp::UdpTransport::new(args.node_id, 50000 + args.node_id as u16, node_addr_builder.clone()).await;
    let node_addr = node_addr_builder.addr();
    log::info!("Listen on addr {}", node_addr);

    let timer = Arc::new(SystemTimer());
    let router = SharedRouter::new(args.node_id);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id: args.node_id,
        neighbours: args.neighbours.clone(),
        timer: timer.clone(),
    });

    let spreads_layer_router = LayersSpreadRouterSyncBehavior::new(router.clone());
    let key_value_sdk = key_value::KeyValueSdk::new();
    let key_value = key_value::KeyValueBehavior::new(args.node_id, timer.clone(), 10000, Some(Box::new(key_value_sdk.clone())));

    if let Some(addr) = args.redis_addr {
        let mut redis_server = RedisServer::new(addr, key_value_sdk);
        async_std::task::spawn(async move {
            redis_server.run().await;
        });
    }

    let mut plan_cfg = NetworkPlaneConfig {
        router: Arc::new(router),
        node_id: args.node_id,
        tick_ms: 1000,
        behaviors: vec![Box::new(manual), Box::new(spreads_layer_router), Box::new(key_value)],
        transport: Box::new(transport),
        timer,
    };

    if args.tun_tap {
        let tun_tap = tun_tap::TunTapBehavior::default();
        plan_cfg.behaviors.push(Box::new(tun_tap));
    }

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(plan_cfg);

    plane.started();

    while let Ok(_) = plane.recv().await {}

    plane.stopped();
}

use atm0s_sdn::convert_enum;
use atm0s_sdn::SharedRouter;
use atm0s_sdn::SystemTimer;
use atm0s_sdn::{KeyValueBehavior, KeyValueSdk, NodeAddr, NodeAddrBuilder, UdpTransport};
use atm0s_sdn::{KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent};
use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use atm0s_sdn::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
use atm0s_sdn::{NetworkPlane, NetworkPlaneConfig};
use atm0s_sdn_redis_server::RedisServer;
use atm0s_sdn_tun_tap::{TunTapBehaviorEvent, TunTapHandlerEvent};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;

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

    /// Neighbours
    #[arg(env, long)]
    seeds: Vec<NodeAddr>,

    /// Local tags
    #[arg(env, long)]
    tags: Vec<String>,

    /// Tags of nodes to connect
    #[arg(env, long)]
    connect_tags: Vec<String>,

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
    node_addr_builder.set_node_id(args.node_id);
    let transport = UdpTransport::new(args.node_id, 50000 + args.node_id as u16, node_addr_builder.clone()).await;
    let node_addr = node_addr_builder.addr();
    log::info!("Listen on addr {}", node_addr);

    let timer = Arc::new(SystemTimer());
    let router = SharedRouter::new(args.node_id);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id: args.node_id,
        node_addr,
        seeds: args.seeds.clone(),
        local_tags: args.tags,
        connect_tags: args.connect_tags,
    });

    let spreads_layer_router: LayersSpreadRouterSyncBehavior = LayersSpreadRouterSyncBehavior::new(router.clone());
    let key_value_sdk = KeyValueSdk::new();
    let key_value = KeyValueBehavior::new(args.node_id, 10000, Some(Box::new(key_value_sdk.clone())));

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
        let tun_tap: atm0s_sdn_tun_tap::TunTapBehavior<_, _> = atm0s_sdn_tun_tap::TunTapBehavior::default();
        plan_cfg.behaviors.push(Box::new(tun_tap));
    }

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(plan_cfg);

    plane.started();

    while let Ok(_) = plane.recv().await {}

    plane.stopped();
}

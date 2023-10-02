use bluesea_identity::{NodeAddr, NodeAddrBuilder, Protocol};
use clap::Parser;
use layers_spread_router::SharedRouter;
use layers_spread_router_sync::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, LayersSpreadRouterSyncReq, LayersSpreadRouterSyncRes};
use manual_discovery::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, ManualReq, ManualRes};
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use std::sync::Arc;
use tun_tap::{TunTapBehaviorEvent, TunTapHandlerEvent, TunTapReq, TunTapRes};
use utils::SystemTimer;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncBehaviorEvent),
    TunTap(TunTapBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncHandlerEvent),
    TunTap(TunTapHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeRpcReq {
    Manual(ManualReq),
    LayersSpreadRouterSync(LayersSpreadRouterSyncReq),
    TunTap(TunTapReq),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeRpcRes {
    Manual(ManualRes),
    LayersSpreadRouterSync(LayersSpreadRouterSyncRes),
    TunTap(TunTapRes),
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
}

#[async_std::main]
async fn main() {
    env_logger::builder().format_timestamp_millis().init();
    let args: Args = Args::parse();
    let node_addr_builder = Arc::new(NodeAddrBuilder::default());
    node_addr_builder.add_protocol(Protocol::P2p(args.node_id));
    let transport = transport_udp::UdpTransport::new(args.node_id, 50000 + args.node_id as u16, node_addr_builder.clone()).await;
    let (transport_rpc, _, _) = network::mock::MockTransportRpc::new();
    let node_addr = node_addr_builder.addr();
    log::info!("Listen on addr {}", node_addr);

    let router = SharedRouter::new(args.node_id);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        neighbours: args.neighbours.clone(),
        timer: Arc::new(SystemTimer()),
    });

    let tun_tap = tun_tap::TunTapBehavior::default();
    let spreads_layer_router = LayersSpreadRouterSyncBehavior::new(router.clone());

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeRpcReq, NodeRpcRes>::new(NetworkPlaneConfig {
        router: Arc::new(router),
        local_node_id: args.node_id,
        tick_ms: 1000,
        behavior: vec![Box::new(manual), Box::new(spreads_layer_router), Box::new(tun_tap)],
        transport: Box::new(transport),
        transport_rpc: Box::new(transport_rpc),
        timer: Arc::new(SystemTimer()),
    });

    plane.started();

    while let Ok(_) = plane.recv().await {}

    plane.stopped();
}

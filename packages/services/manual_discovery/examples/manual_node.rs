use bluesea_identity::{NodeAddr, NodeAddrBuilder, Protocol};
use clap::Parser;
use layers_spread_router::SharedRouter;
use layers_spread_router_sync::*;
use manual_discovery::*;
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utils::SystemTimer;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeSdkEvent {
}

#[derive(convert_enum::From, convert_enum::TryInto, Serialize, Deserialize)]
enum NodeMsg {
    Manual(ManualMsg),
    LayersSpreadRouterSync(LayersSpreadRouterSyncMsg),
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
    env_logger::init();
    let args: Args = Args::parse();
    let node_addr_builder = Arc::new(NodeAddrBuilder::default());
    node_addr_builder.add_protocol(Protocol::P2p(args.node_id));
    let transport = transport_tcp::TcpTransport::new(args.node_id, 0, node_addr_builder.clone()).await;
    let node_addr = node_addr_builder.addr();
    log::info!("Listen on addr {}", node_addr);

    let router = SharedRouter::new(args.node_id);
    let spreads_layer_router = LayersSpreadRouterSyncBehavior::new(router.clone());

    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id: args.node_id,
        neighbours: args.neighbours.clone(),
        timer: Arc::new(SystemTimer()),
    });

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(NetworkPlaneConfig {
        node_id: args.node_id,
        tick_ms: 1000,
        behaviors: vec![Box::new(spreads_layer_router), Box::new(manual)],
        transport: Box::new(transport),
        timer: Arc::new(SystemTimer()),
        router: Arc::new(router),
    });

    plane.started();
    while let Ok(_) = plane.recv().await {}
    plane.stopped();
}

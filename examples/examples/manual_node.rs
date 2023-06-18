use bluesea_identity::{NodeAddr, NodeAddrBuilder, Protocol};
use clap::Parser;
use fast_path_route::{FastPathRouteBehavior, FastPathRouteBehaviorEvent, FastPathRouteHandlerEvent, FastPathRouteMsg, FastPathRouteReq, FastPathRouteRes};
use manual::*;
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use router::SharedRouter;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utils::SystemTimer;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    FastPathRoute(FastPathRouteBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    FastPathRoute(FastPathRouteHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto, Serialize, Deserialize)]
enum NodeMsg {
    Manual(ManualMsg),
    FastPathRoute(FastPathRouteMsg),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeRpcReq {
    Manual(ManualReq),
    FastPathRoute(FastPathRouteReq),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeRpcRes {
    Manual(ManualRes),
    FastPathRoute(FastPathRouteRes),
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
    let transport = transport_tcp::TcpTransport::<NodeMsg>::new(args.node_id, 0, node_addr_builder.clone()).await;
    let (transport_rpc, _, _) = network::mock::MockTransportRpc::new();
    let node_addr = node_addr_builder.addr();
    log::info!("Listen on addr {}", node_addr);

    let router = SharedRouter::new(args.node_id);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        neighbours: args.neighbours.clone(),
        timer: Arc::new(SystemTimer()),
    });

    let fast_path_route = FastPathRouteBehavior::new(router.clone());

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeMsg, NodeRpcReq, NodeRpcRes>::new(NetworkPlaneConfig {
        local_node_id: args.node_id,
        tick_ms: 1000,
        behavior: vec![Box::new(manual), Box::new(fast_path_route)],
        transport: Box::new(transport),
        transport_rpc: Box::new(transport_rpc),
        timer: Arc::new(SystemTimer()),
    });

    while let Ok(e) = plane.recv().await {}
}

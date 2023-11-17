use atm0s_sdn::convert_enum;
use atm0s_sdn::SharedRouter;
use atm0s_sdn::SystemTimer;
use atm0s_sdn::{
    LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, NodeAddr,
    NodeAddrBuilder, Protocol, UdpTransport,
};
use atm0s_sdn::{NetworkPlane, NetworkPlaneConfig};
use clap::Parser;
use std::sync::Arc;

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
enum NodeSdkEvent {}

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
    let transport = UdpTransport::new(args.node_id, 0, node_addr_builder.clone()).await;
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

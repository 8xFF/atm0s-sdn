use atm0s_sdn::convert_enum;
use atm0s_sdn::KeyValueBehavior;
use atm0s_sdn::KeyValueBehaviorEvent;
use atm0s_sdn::KeyValueHandlerEvent;
use atm0s_sdn::KeyValueSdkEvent;
use atm0s_sdn::SharedRouter;
use atm0s_sdn::SystemTimer;
use atm0s_sdn::{
    LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, NodeAddr,
    NodeAddrBuilder, UdpTransport,
};
use atm0s_sdn::{NetworkPlane, NetworkPlaneConfig};
use clap::Parser;
use std::sync::Arc;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Manual(ManualBehaviorEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncBehaviorEvent),
    KeyValue(KeyValueBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandleEvent {
    Manual(ManualHandlerEvent),
    LayersSpreadRouterSync(LayersSpreadRouterSyncHandlerEvent),
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
    seeds: Vec<NodeAddr>,
}

#[async_std::main]
async fn main() {
    env_logger::init();
    let args: Args = Args::parse();
    let mut node_addr_builder = NodeAddrBuilder::new(args.node_id);
    let socket = UdpTransport::prepare(0, &mut node_addr_builder).await;
    let transport = Box::new(UdpTransport::new(node_addr_builder.addr(), socket));
    let node_addr = node_addr_builder.addr();
    log::info!("Listen on addr {}", node_addr);

    let router = SharedRouter::new(args.node_id);
    let spreads_layer_router = LayersSpreadRouterSyncBehavior::new(router.clone());
    let key_value = KeyValueBehavior::<NodeHandleEvent, NodeSdkEvent>::new(args.node_id, 3000, None);

    let manual = ManualBehavior::new(ManualBehaviorConf {
        node_id: args.node_id,
        node_addr: node_addr_builder.addr(),
        seeds: args.seeds,
        local_tags: vec![],
        connect_tags: vec![],
    });

    let mut plane = NetworkPlane::<NodeBehaviorEvent, NodeHandleEvent, NodeSdkEvent>::new(NetworkPlaneConfig {
        node_id: args.node_id,
        tick_ms: 1000,
        behaviors: vec![Box::new(spreads_layer_router), Box::new(key_value), Box::new(manual)],
        transport,
        timer: Arc::new(SystemTimer()),
        router: Arc::new(router),
    });

    plane.started();
    while let Ok(_) = plane.recv().await {}
    plane.stopped();
}

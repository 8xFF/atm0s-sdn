use bluesea_identity::NodeId;
use discovery::{
    DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg, DiscoveryNetworkBehavior,
    DiscoveryNetworkBehaviorOpts,
};
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use std::sync::Arc;
use transport_vnet::VnetEarth;
use utils::SystemTimer;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Discovery(DiscoveryBehaviorEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandlerEvent {
    Discovery(DiscoveryHandlerEvent),
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeMessage {
    Discovery(DiscoveryMsg),
}

async fn start_node(node_id: NodeId, earth: Arc<VnetEarth<NodeMessage>>) {
    // let timer = Arc::new(SystemTimer());
    //
    // let behavior = Box::new(DiscoveryNetworkBehavior::new(
    //     DiscoveryNetworkBehaviorOpts {
    //         local_node_id: node_id,
    //         bootstrap_addrs: Some(vec![(neighbour1, neighbour1_addr.clone())]),
    //         timer: timer.clone(),
    //     },
    // ));
    //
    // let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent, ImplNetworkMsg>::new(
    //     NetworkPlaneConfig {
    //         local_node_id: 0,
    //         tick_ms: 100,
    //         behavior: vec![behavior],
    //         transport,
    //         timer,
    //     },
    // );
    //
    // let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });
}

#[async_std::main]
async fn main() {
    env_logger::init();
    //let earth = Arc::new(VnetEarth::default());
}

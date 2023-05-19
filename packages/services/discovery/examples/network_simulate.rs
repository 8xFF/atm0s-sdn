use std::sync::Arc;
use bluesea_identity::PeerId;
use discovery::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg, DiscoveryNetworkBehavior, DiscoveryNetworkBehaviorOpts};
use transport_vnet::VnetEarth;
use network::convert_enum;
use network::plane::{NetworkPlane, NetworkPlaneConfig};
use utils::SystemTimer;

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeBehaviorEvent {
    Discovery(DiscoveryBehaviorEvent)
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeHandlerEvent {
    Discovery(DiscoveryHandlerEvent)
}

#[derive(convert_enum::From, convert_enum::TryInto)]
enum NodeMessage {
    Discovery(DiscoveryMsg)
}

async fn start_node(node_id: PeerId, earth: Arc<VnetEarth<NodeMessage>>) {
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
    //         local_peer_id: 0,
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
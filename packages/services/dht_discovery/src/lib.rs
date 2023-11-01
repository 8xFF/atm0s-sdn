pub static DISCOVERY_SERVICE_ID: u8 = 0;

mod behavior;
mod connection_group;
mod find_key_request;
mod handler;
pub(crate) mod kbucket;
mod logic;
mod msg;

pub use behavior::{DiscoveryNetworkBehavior, DiscoveryNetworkBehaviorOpts};
pub use msg::*;

#[cfg(test)]
mod tests {
    // use crate::behavior::{DiscoveryNetworkBehavior, DiscoveryNetworkBehaviorOpts};
    // use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg};
    // use crate::DISCOVERY_SERVICE_ID;
    // use bluesea_identity::{ConnId, NodeAddr, Protocol};
    // use bluesea_router::{ForceNodeRouter, RouteRule};
    // use network::convert_enum;
    // use network::mock::{MockInput, MockOutput, MockTransport};
    // use network::msg::TransportMsg;
    // use network::plane::{NetworkPlane, NetworkPlaneConfig};
    // use std::sync::Arc;
    // use std::time::Duration;
    // use utils::option_handle::OptionUtils;
    // use utils::SystemTimer;

    // #[derive(convert_enum::From, convert_enum::TryInto, PartialEq, Debug)]
    // enum ImplNetworkMsg {
    //     Discovery(DiscoveryMsg),
    // }

    // #[derive(convert_enum::From, convert_enum::TryInto)]
    // enum ImplBehaviorEvent {
    //     Discovery(DiscoveryBehaviorEvent),
    // }

    // #[derive(convert_enum::From, convert_enum::TryInto)]
    // enum ImplHandlerEvent {
    //     Discovery(DiscoveryHandlerEvent),
    // }
    // #[async_std::test]
    // async fn bootstrap() {
    //     let neighbour1 = 1000;
    //     let neighbour1_addr = NodeAddr::from(Protocol::Memory(1000));

    //     let (mock, faker, output) = MockTransport::new();
    //     let transport = Box::new(mock);
    //     let timer = Arc::new(SystemTimer());

    //     let behavior = Box::new(DiscoveryNetworkBehavior::new(DiscoveryNetworkBehaviorOpts {
    //         local_node_id: 0,
    //         bootstrap_addrs: Some(vec![(neighbour1, neighbour1_addr.clone())]),
    //         timer: timer.clone(),
    //     }));

    //     let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent>::new(NetworkPlaneConfig {
    //         local_node_id: 0,
    //         tick_ms: 100,
    //         behavior: vec![behavior],
    //         transport,
    //         timer,
    //         router: Arc::new(ForceNodeRouter(ConnId::from_out(0, 0), neighbour1)),
    //     });

    //     let join = async_std::task::spawn(async move {
    //         plane.started();
    //         while let Ok(_) = plane.recv().await {}
    //         plane.stopped();
    //     });

    //     async_std::task::sleep(Duration::from_millis(1000)).await;
    //     assert_eq!(output.lock().pop_front(), Some(MockOutput::ConnectTo(neighbour1, neighbour1_addr.clone())));
    //     faker
    //         .send_blocking(MockInput::FakeOutgoingConnection(neighbour1, ConnId::from_out(0, 0), neighbour1_addr.clone()))
    //         .unwrap();
    //     async_std::task::sleep(Duration::from_millis(100)).await;
    //     assert_eq!(
    //         output.lock().pop_front(),
    //         Some(MockOutput::SendTo(
    //             neighbour1,
    //             ConnId::from_out(0, 0),
    //             TransportMsg::build_reliable(DISCOVERY_SERVICE_ID, RouteRule::ToNode(neighbour1), 0, &bincode::serialize(&DiscoveryMsg::FindKey(0, 0)).unwrap())
    //         ))
    //     );
    //     join.cancel().await.print_none("Should cancel join");
    // }

    // #[async_std::test]
    // async fn auto_refresh() {
    //     let neighbour1 = 1000;
    //     let neighbour1_addr = NodeAddr::from(Protocol::Memory(1000));

    //     let (mock, faker, output) = MockTransport::new();
    //     let transport = Box::new(mock);
    //     let timer = Arc::new(SystemTimer());

    //     let behavior = Box::new(DiscoveryNetworkBehavior::new(DiscoveryNetworkBehaviorOpts {
    //         local_node_id: 0,
    //         bootstrap_addrs: None,
    //         timer: timer.clone(),
    //     }));

    //     let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent>::new(NetworkPlaneConfig {
    //         local_node_id: 0,
    //         tick_ms: 100,
    //         behavior: vec![behavior],
    //         transport,
    //         timer,
    //         router: Arc::new(ForceNodeRouter(ConnId::from_in(0, 0), neighbour1)),
    //     });

    //     let join = async_std::task::spawn(async move {
    //         plane.started();
    //         while let Ok(_) = plane.recv().await {}
    //         plane.stopped();
    //     });

    //     faker
    //         .send_blocking(MockInput::FakeIncomingConnection(neighbour1, ConnId::from_in(0, 0), neighbour1_addr.clone()))
    //         .unwrap();
    //     async_std::task::sleep(Duration::from_millis(200)).await;
    //     assert_eq!(
    //         output.lock().pop_front(),
    //         Some(MockOutput::SendTo(
    //             neighbour1,
    //             ConnId::from_in(0, 0),
    //             TransportMsg::build_reliable(DISCOVERY_SERVICE_ID, RouteRule::ToNode(neighbour1), 0, &bincode::serialize(&DiscoveryMsg::FindKey(0, 0)).unwrap())
    //         ))
    //     );
    //     join.cancel().await.print_none("Should cancel join");
    // }
}

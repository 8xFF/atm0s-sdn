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
    use crate::behavior::{DiscoveryNetworkBehavior, DiscoveryNetworkBehaviorOpts};
    use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg};
    use crate::DISCOVERY_SERVICE_ID;
    use bluesea_identity::{PeerAddr, Protocol};
    use network::convert_enum;
    use network::mock::{MockInput, MockOutput, MockTransport, MockTransportRpc};
    use network::plane::{NetworkPlane, NetworkPlaneConfig};
    use network::transport::ConnectionMsg;
    use std::sync::Arc;
    use std::time::Duration;
    use utils::SystemTimer;

    #[derive(convert_enum::From, convert_enum::TryInto, PartialEq, Debug)]
    enum ImplNetworkMsg {
        Discovery(DiscoveryMsg),
    }

    enum ImplNetworkReq {}
    enum ImplNetworkRes {}

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplBehaviorEvent {
        Discovery(DiscoveryBehaviorEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplHandlerEvent {
        Discovery(DiscoveryHandlerEvent),
    }

    #[async_std::test]
    async fn bootstrap() {
        let neighbour1 = 1000;
        let neighbour1_addr = PeerAddr::from(Protocol::Memory(1000));

        let (mock, faker, output) = MockTransport::<ImplNetworkMsg>::new();
        let (mock_rpc, faker_rpc, output_rpc) =
            MockTransportRpc::<ImplNetworkReq, ImplNetworkRes>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let behavior = Box::new(DiscoveryNetworkBehavior::new(
            DiscoveryNetworkBehaviorOpts {
                local_node_id: 0,
                bootstrap_addrs: Some(vec![(neighbour1, neighbour1_addr.clone())]),
                timer: timer.clone(),
            },
        ));

        let mut plane = NetworkPlane::<
            ImplBehaviorEvent,
            ImplHandlerEvent,
            ImplNetworkMsg,
            ImplNetworkReq,
            ImplNetworkRes,
        >::new(NetworkPlaneConfig {
            local_peer_id: 0,
            tick_ms: 100,
            behavior: vec![behavior],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(
            output.lock().pop_front(),
            Some(MockOutput::ConnectTo(neighbour1, neighbour1_addr.clone()))
        );
        faker
            .send_blocking(MockInput::FakeOutgoingConnection(
                neighbour1,
                0,
                neighbour1_addr.clone(),
            ))
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            output.lock().pop_front(),
            Some(MockOutput::SendTo(
                DISCOVERY_SERVICE_ID,
                neighbour1,
                0,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: DiscoveryMsg::FindKey(0, 0).into(),
                }
            ))
        );
        join.cancel();
    }

    #[async_std::test]
    async fn auto_refresh() {
        let neighbour1 = 1000;
        let neighbour1_addr = PeerAddr::from(Protocol::Memory(1000));

        let (mock, faker, output) = MockTransport::<ImplNetworkMsg>::new();
        let (mock_rpc, faker_rpc, output_rpc) =
            MockTransportRpc::<ImplNetworkReq, ImplNetworkRes>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let behavior = Box::new(DiscoveryNetworkBehavior::new(
            DiscoveryNetworkBehaviorOpts {
                local_node_id: 0,
                bootstrap_addrs: None,
                timer: timer.clone(),
            },
        ));

        let mut plane = NetworkPlane::<
            ImplBehaviorEvent,
            ImplHandlerEvent,
            ImplNetworkMsg,
            ImplNetworkReq,
            ImplNetworkRes,
        >::new(NetworkPlaneConfig {
            local_peer_id: 0,
            tick_ms: 100,
            behavior: vec![behavior],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });
        faker
            .send_blocking(MockInput::FakeIncomingConnection(
                neighbour1,
                0,
                neighbour1_addr.clone(),
            ))
            .unwrap();
        async_std::task::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            output.lock().pop_front(),
            Some(MockOutput::SendTo(
                DISCOVERY_SERVICE_ID,
                neighbour1,
                0,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: DiscoveryMsg::FindKey(0, 0).into(),
                }
            ))
        );
        join.cancel();
    }
}

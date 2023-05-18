pub static DISCOVERY_SERVICE_ID: u8 = 0;

mod behavior;
mod find_key_request;
mod connection_group;
mod handler;
pub(crate) mod kbucket;
mod logic;
mod msg;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::PeerAddr;
    use network::mock::{MockInput, MockOutput, MockTransport};
    use network::plane::{NetworkPlane, NetworkPlaneConfig};
    use network::convert_enum;
    use network::transport::ConnectionMsg;
    use utils::SystemTimer;
    use crate::behavior::{DiscoveryNetworkBehavior, DiscoveryNetworkBehaviorOpts};
    use crate::DISCOVERY_SERVICE_ID;
    use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent, DiscoveryMsg};

    #[derive(convert_enum::From, convert_enum::TryInto, PartialEq, Debug)]
    enum ImplNetworkMsg {
        Discovery(DiscoveryMsg)
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplBehaviorEvent {
        Discovery(DiscoveryBehaviorEvent)
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplHandlerEvent {
        Discovery(DiscoveryHandlerEvent)
    }

    #[async_std::test]
    async fn bootstrap() {
        let neighbour1 = 1000;
        let neighbour1_addr = PeerAddr::from(Protocol::Memory(1000));

        let (mock, faker, output) = MockTransport::<ImplNetworkMsg>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let behavior = Box::new(DiscoveryNetworkBehavior::new(DiscoveryNetworkBehaviorOpts {
            local_node_id: 0,
            bootstrap_addrs: Some(vec![(neighbour1, neighbour1_addr.clone())]),
            timer: timer.clone()
        }));

        let mut plane = NetworkPlane::<
            ImplBehaviorEvent,
            ImplHandlerEvent,
            ImplNetworkMsg,
        >::new(NetworkPlaneConfig {
            local_peer_id: 0,
            tick_ms: 100,
            behavior: vec![behavior],
            transport,
            timer,
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(
            output.lock().pop_front(),
            Some(MockOutput::ConnectTo(neighbour1, neighbour1_addr.clone()))
        );
        faker.send_blocking(MockInput::FakeOutgoingConnection(neighbour1, 0, neighbour1_addr.clone())).unwrap();
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
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let behavior = Box::new(DiscoveryNetworkBehavior::new(DiscoveryNetworkBehaviorOpts {
            local_node_id: 0,
            bootstrap_addrs: None,
            timer: timer.clone()
        }));

        let mut plane = NetworkPlane::<
            ImplBehaviorEvent,
            ImplHandlerEvent,
            ImplNetworkMsg,
        >::new(NetworkPlaneConfig {
            local_peer_id: 0,
            tick_ms: 100,
            behavior: vec![behavior],
            transport,
            timer,
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });
        faker.send_blocking(MockInput::FakeIncomingConnection(neighbour1, 0, neighbour1_addr.clone())).unwrap();
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

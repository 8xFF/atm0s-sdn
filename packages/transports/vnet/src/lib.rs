mod connection;
mod connector;
mod earth;
mod listener;
mod transport;

pub const VNET_PROTOCOL_ID: u8 = 1;
pub use earth::VnetEarth;
pub use transport::VnetTransport;

#[cfg(test)]
mod tests {
    use crate::{VnetEarth, VnetTransport};
    use atm0s_sdn_identity::{ConnDirection, NodeAddr, NodeId, Protocol};
    use atm0s_sdn_network::{
        msg::TransportMsg,
        transport::{ConnectionEvent, ConnectionStats, OutgoingConnectionError, Transport, TransportEvent},
    };
    use atm0s_sdn_router::RouteRule;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    #[derive(PartialEq, Debug, Serialize, Deserialize)]
    enum Msg {
        Ping,
        Pong,
    }

    fn build_msg(to_node: NodeId, msg: Msg) -> TransportMsg {
        TransportMsg::build(0, 0, RouteRule::ToNode(to_node), 0, 0, &bincode::serialize(&msg).unwrap())
    }

    #[async_std::test]
    async fn simple_network() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::new(vnet.clone(), 1, 1, NodeAddr::from(Protocol::Memory(1)));
        let mut tran2 = VnetTransport::new(vnet.clone(), 2, 2, NodeAddr::from(Protocol::Memory(2)));

        let connector1 = tran1.connector();
        connector1.connect_to(11111, 2, NodeAddr::from(Protocol::Memory(2))).unwrap();

        match tran2.recv().await.unwrap() {
            TransportEvent::IncomingRequest(node, conn, acceptor) => {
                assert_eq!(node, 1);
                assert_eq!(conn.direction(), ConnDirection::Incoming);
                acceptor.accept();
            }
            _ => {
                panic!("Need IncomingRequest")
            }
        }

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(node, _conn, acceptor, local_uuid) => {
                assert_eq!(node, 2);
                assert_eq!(local_uuid, 11111);
                acceptor.accept();
            }
            _ => {
                panic!("Need OutgoingRequest")
            }
        }

        let (tran2_sender, mut tran2_recv) = match tran2.recv().await.unwrap() {
            TransportEvent::Incoming(sender, recv) => {
                assert_eq!(sender.remote_node_id(), 1);
                assert_eq!(sender.remote_addr(), NodeAddr::from(Protocol::Memory(1)));
                assert_eq!(sender.conn_id().direction(), ConnDirection::Incoming);
                (sender, recv)
            }
            _ => {
                panic!("Need incoming")
            }
        };

        let (tran1_sender, mut tran1_recv) = match tran1.recv().await.unwrap() {
            TransportEvent::Outgoing(sender, recv, local_uuid) => {
                assert_eq!(sender.remote_node_id(), 2);
                assert_eq!(sender.remote_addr(), NodeAddr::from(Protocol::Memory(2)));
                assert_eq!(local_uuid, 11111);
                (sender, recv)
            }
            _ => {
                panic!("Need outgoing")
            }
        };

        let received_event = tran2_recv.poll().await.unwrap();
        assert_eq!(
            received_event,
            ConnectionEvent::Stats(ConnectionStats {
                rtt_ms: 1,
                sending_kbps: 0,
                send_est_kbps: 100000,
                loss_percent: 0,
                over_use: false,
            })
        );

        let received_event = tran1_recv.poll().await.unwrap();
        assert_eq!(
            received_event,
            ConnectionEvent::Stats(ConnectionStats {
                rtt_ms: 1,
                sending_kbps: 0,
                send_est_kbps: 100000,
                loss_percent: 0,
                over_use: false,
            })
        );

        tran1_sender.send(build_msg(1, Msg::Ping));
        let received_event = tran2_recv.poll().await.unwrap();
        assert_eq!(received_event, ConnectionEvent::Msg(build_msg(1, Msg::Ping)));

        tran2_sender.send(build_msg(1, Msg::Ping));
        let received_event = tran1_recv.poll().await.unwrap();
        assert_eq!(received_event, ConnectionEvent::Msg(build_msg(1, Msg::Ping)));

        tran1_sender.close();
        assert_eq!(tran1_recv.poll().await, Err(()));
        assert_eq!(tran2_recv.poll().await, Err(()));
        assert_eq!(vnet.connections.read().len(), 0);
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::new(vnet.clone(), 1, 1, NodeAddr::from(Protocol::Memory(1)));
        let connector1 = tran1.connector();
        connector1.connect_to(11111, 2, NodeAddr::from(Protocol::Memory(2))).unwrap();
        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingError { err, local_uuid, .. } => {
                assert_eq!(err, OutgoingConnectionError::DestinationNotFound);
                assert_eq!(local_uuid, 11111);
            }
            _ => {
                panic!("Need OutgoingError")
            }
        };
    }

    #[async_std::test]
    async fn simple_network_connect_wrong_node() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::new(vnet.clone(), 1, 1, NodeAddr::from(Protocol::Memory(1)));
        let mut _tran2 = VnetTransport::new(vnet.clone(), 2, 2, NodeAddr::from(Protocol::Memory(2)));
        let connector1 = tran1.connector();
        connector1.connect_to(11111, 3, NodeAddr::from(Protocol::Memory(2))).unwrap();
        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingError { err, local_uuid, .. } => {
                assert_eq!(err, OutgoingConnectionError::AuthenticationError);
                assert_eq!(local_uuid, 11111);
            }
            _ => {
                panic!("Need OutgoingError")
            }
        };
    }
}

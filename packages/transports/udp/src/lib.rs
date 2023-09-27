mod connector;
mod handshake;
mod msg;
mod receiver;
mod sender;
mod transport;

pub const UDP_PROTOCOL_ID: u8 = 3;
pub use transport::UdpTransport;

#[cfg(test)]
mod tests {
    use crate::transport::UdpTransport;
    use bluesea_identity::{NodeAddr, NodeAddrBuilder, Protocol};
    use bluesea_router::RouteRule;
    use network::msg::TransportMsg;
    use network::transport::{ConnectionEvent, OutgoingConnectionError, Transport, TransportEvent};
    use serde::{Deserialize, Serialize};
    use utils::option_handle::OptionUtils;

    use std::net::Ipv4Addr;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(PartialEq, Debug, Serialize, Deserialize)]
    enum Msg {
        Ping,
        Pong,
    }

    fn create_reliable(msg: Msg) -> TransportMsg {
        TransportMsg::build_reliable(0, RouteRule::Direct, 0, &bincode::serialize(&msg).unwrap())
    }

    #[async_std::test]
    async fn simple_network() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let mut tran1 = UdpTransport::new(1, 10001, node_addr_builder1.clone()).await;

        let node_addr_builder2 = Arc::new(NodeAddrBuilder::default());
        let mut tran2 = UdpTransport::new(2, 10002, node_addr_builder2.clone()).await;

        let connector1 = tran1.connector();
        let conn_id = connector1.connect_to(2, node_addr_builder2.addr()).unwrap().conn_id;

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(node, conn, acceptor) => {
                assert_eq!(node, 2);
                assert_eq!(conn, conn_id);
                acceptor.accept();
                log::info!("on outgoing request");
            }
            _ => {
                panic!("Need OutgoingRequest")
            }
        }

        match tran2.recv().await.unwrap() {
            TransportEvent::IncomingRequest(node, _conn, acceptor) => {
                log::info!("on incoming request");
                assert_eq!(node, 1);
                acceptor.accept();
            }
            _ => {
                panic!("Need IncomingRequest")
            }
        }

        let (tran2_sender, mut tran2_recv) = match tran2.recv().await.unwrap() {
            TransportEvent::Incoming(sender, recv) => {
                assert_eq!(sender.remote_node_id(), 1);
                assert_eq!(sender.remote_addr(), node_addr_builder1.addr());
                (sender, recv)
            }
            _ => {
                panic!("Need incoming")
            }
        };

        let (tran1_sender, mut tran1_recv) = match tran1.recv().await.unwrap() {
            TransportEvent::Outgoing(sender, recv) => {
                assert_eq!(sender.remote_node_id(), 2);
                assert_eq!(sender.remote_addr(), node_addr_builder2.addr());
                assert_eq!(sender.conn_id(), conn_id);
                (sender, recv)
            }
            _ => {
                panic!("Need outgoing")
            }
        };

        async_std::task::spawn(async move {
            match tran2_recv.poll().await.unwrap() {
                ConnectionEvent::Stats(_stats) => {}
                e => panic!("Should received stats {:?}", e),
            }
            tran2_sender.send(create_reliable(Msg::Ping));
            let received_event = tran2_recv.poll().await.unwrap();
            assert_eq!(received_event, ConnectionEvent::Msg(create_reliable(Msg::Ping)));
            assert_eq!(tran2_recv.poll().await, Err(()));
        });

        match tran1_recv.poll().await.unwrap() {
            ConnectionEvent::Stats(_stats) => {}
            e => panic!("Should received stats {:?}", e),
        }

        let received_event = tran1_recv.poll().await.unwrap();
        assert_eq!(received_event, ConnectionEvent::Msg(create_reliable(Msg::Ping)));

        tran1_sender.send(create_reliable(Msg::Ping));
        async_std::task::sleep(Duration::from_millis(100)).await;

        tran1_sender.close();
        assert_eq!(tran1_recv.poll().await, Err(()));
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let mut tran1 = UdpTransport::new(1, 20001, node_addr_builder1.clone()).await;
        let connector1 = tran1.connector();
        let _conn_id = connector1
            .connect_to(2, NodeAddr::from_iter(vec![Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)), Protocol::Udp(20002)]))
            .unwrap()
            .conn_id;

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(_node, _conn, acceptor) => {
                acceptor.accept();
            }
            _ => {
                panic!("Need OutgoingRequest")
            }
        }

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingError { err, .. } => {
                assert_eq!(err, OutgoingConnectionError::DestinationNotFound);
            }
            _ => {
                panic!("Need OutgoingError")
            }
        };
    }

    #[async_std::test]
    async fn simple_network_connect_wrong_node() {
        let node_addr_builder1 = Arc::new(NodeAddrBuilder::default());
        let mut tran1 = UdpTransport::new(1, 30001, node_addr_builder1.clone()).await;

        let node_addr_builder2 = Arc::new(NodeAddrBuilder::default());
        let mut tran2 = UdpTransport::new(2, 30002, node_addr_builder2.clone()).await;

        let connector1 = tran1.connector();
        let _conn_id = connector1.connect_to(3, node_addr_builder2.addr()).unwrap().conn_id;

        let join = async_std::task::spawn(async move {
            loop {
                match tran2.recv().await {
                    Ok(msg) => match msg {
                        TransportEvent::IncomingRequest(_, _, acceptor) => {
                            acceptor.accept();
                        }
                        _ => {}
                    },
                    Err(_) => break,
                }
            }
        });

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(_node, _conn, acceptor) => {
                acceptor.accept();
            }
            _ => {
                panic!("Need OutgoingRequest")
            }
        }

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingError { err, .. } => {
                assert_eq!(err, OutgoingConnectionError::DestinationNotFound);
            }
            _ => {
                panic!("Need OutgoingError")
            }
        };

        join.cancel().await.print_none("Should cancel join");
    }
}

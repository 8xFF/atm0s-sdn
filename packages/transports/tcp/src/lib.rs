pub const INCOMING_POSTFIX: u32 = 10;
pub const OUTGOING_POSTFIX: u32 = 11;

mod connection;
mod connector;
mod handshake;
mod msg;
mod transport;

#[cfg(test)]
mod tests {
    use crate::transport::TcpTransport;
    use bluesea_identity::{PeerAddr, PeerAddrBuilder, Protocol};
    use network::transport::{
        ConnectionEvent, ConnectionMsg, OutgoingConnectionError, Transport, TransportEvent,
    };
    use serde::{Deserialize, Serialize};
    use std::net::Ipv4Addr;
    use std::sync::Arc;

    #[derive(PartialEq, Debug, Serialize, Deserialize)]
    enum Msg {
        Ping,
        Pong,
    }

    #[async_std::test]
    async fn simple_network() {
        let peer_addr_builder1 = Arc::new(PeerAddrBuilder::default());
        let mut tran1 = TcpTransport::<Msg>::new(1, 10001, peer_addr_builder1.clone()).await;

        let peer_addr_builder2 = Arc::new(PeerAddrBuilder::default());
        let mut tran2 = TcpTransport::<Msg>::new(2, 10002, peer_addr_builder2.clone()).await;

        let connector1 = tran1.connector();
        let conn_id = connector1
            .connect_to(2, peer_addr_builder2.addr())
            .unwrap()
            .connection_id;

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(peer, conn, acceptor) => {
                assert_eq!(peer, 2);
                assert_eq!(conn, conn_id);
                acceptor.accept();
            }
            _ => {
                panic!("Need OutgoingRequest")
            }
        }

        match tran2.recv().await.unwrap() {
            TransportEvent::IncomingRequest(peer, conn, acceptor) => {
                assert_eq!(peer, 1);
                acceptor.accept();
            }
            _ => {
                panic!("Need IncomingRequest")
            }
        }

        let (tran2_sender, mut tran2_recv) = match tran2.recv().await.unwrap() {
            TransportEvent::Incoming(sender, recv) => {
                assert_eq!(sender.remote_peer_id(), 1);
                assert_eq!(sender.remote_addr(), peer_addr_builder1.addr());
                (sender, recv)
            }
            _ => {
                panic!("Need incoming")
            }
        };

        let (tran1_sender, mut tran1_recv) = match tran1.recv().await.unwrap() {
            TransportEvent::Outgoing(sender, recv) => {
                assert_eq!(sender.remote_peer_id(), 2);
                assert_eq!(sender.remote_addr(), peer_addr_builder2.addr());
                assert_eq!(sender.connection_id(), conn_id);
                (sender, recv)
            }
            _ => {
                panic!("Need outgoing")
            }
        };

        tran1_sender.send(
            0,
            ConnectionMsg::Reliable {
                stream_id: 0,
                data: Msg::Ping,
            },
        );
        let received_event = tran2_recv.poll().await.unwrap();
        assert_eq!(
            received_event,
            ConnectionEvent::Msg {
                service_id: 0,
                msg: ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: Msg::Ping
                }
            }
        );

        tran2_sender.send(
            1,
            ConnectionMsg::Reliable {
                stream_id: 0,
                data: Msg::Ping,
            },
        );
        let received_event = tran1_recv.poll().await.unwrap();
        assert_eq!(
            received_event,
            ConnectionEvent::Msg {
                service_id: 1,
                msg: ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: Msg::Ping
                }
            }
        );

        tran1_sender.close();
        assert_eq!(tran2_recv.poll().await, Err(()));
        assert_eq!(tran1_recv.poll().await, Err(()));
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let peer_addr_builder1 = Arc::new(PeerAddrBuilder::default());
        let mut tran1 = TcpTransport::<Msg>::new(1, 20001, peer_addr_builder1.clone()).await;
        let connector1 = tran1.connector();
        let conn_id = connector1
            .connect_to(
                2,
                PeerAddr::from_iter(vec![
                    Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)),
                    Protocol::Tcp(20002),
                ]),
            )
            .unwrap()
            .connection_id;

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(peer, conn, acceptor) => {
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
            e => {
                panic!("Need OutgoingError")
            }
        };
    }

    #[async_std::test]
    async fn simple_network_connect_wrong_peer() {
        let peer_addr_builder1 = Arc::new(PeerAddrBuilder::default());
        let mut tran1 = TcpTransport::<Msg>::new(1, 30001, peer_addr_builder1.clone()).await;

        let peer_addr_builder2 = Arc::new(PeerAddrBuilder::default());
        let mut tran2 = TcpTransport::<Msg>::new(2, 30002, peer_addr_builder2.clone()).await;

        let connector1 = tran1.connector();
        let conn_id = connector1
            .connect_to(3, peer_addr_builder2.addr())
            .unwrap()
            .connection_id;

        let join = async_std::task::spawn(async move { while tran2.recv().await.is_ok() {} });

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(peer, conn, acceptor) => {
                acceptor.accept();
            }
            _ => {
                panic!("Need OutgoingRequest")
            }
        }

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingError { err, .. } => {
                assert_eq!(err, OutgoingConnectionError::AuthenticationError);
            }
            _ => {
                panic!("Need OutgoingError")
            }
        };

        join.cancel();
    }
}

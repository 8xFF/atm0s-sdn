mod connection;
mod connector;
mod earth;
mod listener;
mod transport;

pub use earth::VnetEarth;
pub use transport::VnetTransport;

#[cfg(test)]
mod tests {
    use crate::{VnetEarth, VnetTransport};
    use bluesea_identity::{NodeAddr, Protocol};
    use network::transport::{ConnectionEvent, ConnectionMsg, MsgRoute, OutgoingConnectionError, Transport, TransportEvent};
    use std::sync::Arc;

    #[derive(PartialEq, Debug)]
    enum Msg {
        Ping,
        Pong,
    }

    #[async_std::test]
    async fn simple_network() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::<Msg>::new(vnet.clone(), 1, 1, NodeAddr::from(Protocol::Memory(1)));
        let mut tran2 = VnetTransport::<Msg>::new(vnet.clone(), 2, 2, NodeAddr::from(Protocol::Memory(2)));

        let connector1 = tran1.connector();
        let conn_id = connector1.connect_to(2, NodeAddr::from(Protocol::Memory(2))).unwrap().conn_id;

        match tran2.recv().await.unwrap() {
            TransportEvent::IncomingRequest(node, conn, acceptor) => {
                assert_eq!(node, 1);
                assert_eq!(conn, conn_id);
                acceptor.accept();
            }
            _ => {
                panic!("Need IncomingRequest")
            }
        }

        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingRequest(node, conn, acceptor) => {
                assert_eq!(node, 2);
                assert_eq!(conn, conn_id);
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
                assert_eq!(sender.conn_id(), conn_id);
                (sender, recv)
            }
            _ => {
                panic!("Need incoming")
            }
        };

        let (tran1_sender, mut tran1_recv) = match tran1.recv().await.unwrap() {
            TransportEvent::Outgoing(sender, recv) => {
                assert_eq!(sender.remote_node_id(), 2);
                assert_eq!(sender.remote_addr(), NodeAddr::from(Protocol::Memory(2)));
                assert_eq!(sender.conn_id(), conn_id);
                (sender, recv)
            }
            _ => {
                panic!("Need outgoing")
            }
        };

        tran1_sender.send(MsgRoute::Node(1), 1, 0, ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping });
        let received_event = tran2_recv.poll().await.unwrap();
        assert_eq!(
            received_event,
            ConnectionEvent::Msg {
                route: MsgRoute::Node(1),
                ttl: 1,
                service_id: 0,
                msg: ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping }
            }
        );

        tran2_sender.send(MsgRoute::Node(1), 1, 0, ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping });
        let received_event = tran1_recv.poll().await.unwrap();
        assert_eq!(
            received_event,
            ConnectionEvent::Msg {
                route: MsgRoute::Node(1),
                ttl: 1,
                service_id: 1,
                msg: ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping }
            }
        );

        tran1_sender.close();
        assert_eq!(tran1_recv.poll().await, Err(()));
        assert_eq!(tran2_recv.poll().await, Err(()));
        assert_eq!(vnet.connections.read().len(), 0);
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::<Msg>::new(vnet.clone(), 1, 1, NodeAddr::from(Protocol::Memory(1)));
        let connector1 = tran1.connector();
        let conn_id = connector1.connect_to(2, NodeAddr::from(Protocol::Memory(2))).unwrap().conn_id;
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
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::<Msg>::new(vnet.clone(), 1, 1, NodeAddr::from(Protocol::Memory(1)));
        let mut tran2 = VnetTransport::<Msg>::new(vnet.clone(), 2, 2, NodeAddr::from(Protocol::Memory(2)));
        let connector1 = tran1.connector();
        let conn_id = connector1.connect_to(3, NodeAddr::from(Protocol::Memory(2))).unwrap().conn_id;
        match tran1.recv().await.unwrap() {
            TransportEvent::OutgoingError { err, .. } => {
                assert_eq!(err, OutgoingConnectionError::AuthenticationError);
            }
            _ => {
                panic!("Need OutgoingError")
            }
        };
    }
}

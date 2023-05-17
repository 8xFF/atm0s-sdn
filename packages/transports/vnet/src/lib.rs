mod transport;
mod connector;
mod connection;
mod earth;
mod listener;

pub use earth::VnetEarth;
pub use transport::VnetTransport;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::PeerAddr;
    use network::transport::{ConnectionEvent, ConnectionMsg, OutgoingConnectionError, Transport, TransportEvent};
    use crate::{VnetEarth, VnetTransport};

    #[derive(PartialEq, Debug)]
    enum Msg {
        Ping,
        Pong,
    }

    #[async_std::test]
    async fn simple_network() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::<Msg>::new(vnet.clone(), 1, 1, PeerAddr::from(Protocol::Memory(1)));
        let mut tran2 = VnetTransport::<Msg>::new(vnet.clone(), 2, 2, PeerAddr::from(Protocol::Memory(2)));

        let connector1 = tran1.connector();
        let conn_id = connector1.connect_to(2, PeerAddr::from(Protocol::Memory(2))).unwrap().connection_id;

        let (tran2_sender, mut tran2_recv) = match tran2.recv().await.unwrap() {
            TransportEvent::Incoming(sender, recv) => {
                assert_eq!(sender.remote_peer_id(), 1);
                assert_eq!(sender.remote_addr(), PeerAddr::from(Protocol::Memory(1)));
                assert_eq!(sender.connection_id(), conn_id);
                (sender, recv)
            }
            TransportEvent::Outgoing(_, _) => {
                panic!("Need incoming")
            }
            TransportEvent::OutgoingError { .. } => {
                panic!("Need incoming")
            }
        };

        let (tran1_sender, mut tran1_recv) = match tran1.recv().await.unwrap() {
            TransportEvent::Outgoing(sender, recv) => {
                assert_eq!(sender.remote_peer_id(), 2);
                assert_eq!(sender.remote_addr(), PeerAddr::from(Protocol::Memory(2)));
                assert_eq!(sender.connection_id(), conn_id);
                (sender, recv)
            }
            TransportEvent::Incoming(_, _) => {
                panic!("Need outgoing")
            }
            TransportEvent::OutgoingError { .. } => {
                panic!("Need outgoing")
            }
        };

        tran1_sender.send(0, ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping });
        let received_event = tran2_recv.poll().await.unwrap();
        assert_eq!(received_event, ConnectionEvent::Msg { service_id: 0, msg: ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping } });

        tran2_sender.send(1, ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping });
        let received_event = tran1_recv.poll().await.unwrap();
        assert_eq!(received_event, ConnectionEvent::Msg { service_id: 1, msg: ConnectionMsg::Reliable { stream_id: 0, data: Msg::Ping } });

        tran1_sender.close();
        assert_eq!(tran1_recv.poll().await, Err(()));
        assert_eq!(tran2_recv.poll().await, Err(()));
    }

    #[async_std::test]
    async fn simple_network_connect_addr_not_found() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::<Msg>::new(vnet.clone(), 1, 1, PeerAddr::from(Protocol::Memory(1)));
        let connector1 = tran1.connector();
        let conn_id = connector1.connect_to(2, PeerAddr::from(Protocol::Memory(2))).unwrap().connection_id;
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
    async fn simple_network_connect_wrong_peer() {
        let vnet = Arc::new(VnetEarth::default());

        let mut tran1 = VnetTransport::<Msg>::new(vnet.clone(), 1, 1, PeerAddr::from(Protocol::Memory(1)));
        let mut tran2 = VnetTransport::<Msg>::new(vnet.clone(), 2, 2, PeerAddr::from(Protocol::Memory(2)));
        let connector1 = tran1.connector();
        let conn_id = connector1.connect_to(3, PeerAddr::from(Protocol::Memory(2))).unwrap().connection_id;
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

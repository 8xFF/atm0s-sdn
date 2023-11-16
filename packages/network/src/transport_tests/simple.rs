use p_8xff_sdn_identity::NodeAddr;
use p_8xff_sdn_router::RouteRule;
use p_8xff_sdn_utils::option_handle::OptionUtils;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    msg::TransportMsg,
    transport::{ConnectionEvent, OutgoingConnectionError, Transport, TransportEvent},
};

#[derive(PartialEq, Debug, Serialize, Deserialize)]
enum Msg {
    Ping,
    Pong,
}

fn create_reliable(msg: Msg) -> TransportMsg {
    TransportMsg::build_reliable(0, RouteRule::Direct, 0, &bincode::serialize(&msg).unwrap())
}

pub async fn simple_network<T: Transport>(mut tran1: T, node1_addr: NodeAddr, mut tran2: T, node2_addr: NodeAddr) {
    let connector1 = tran1.connector();
    connector1.connect_to(1111, 2, node2_addr.clone()).unwrap();

    match tran1.recv().await.unwrap() {
        TransportEvent::OutgoingRequest(node, _conn, acceptor, local_uuid) => {
            assert_eq!(node, 2);
            assert_eq!(local_uuid, 1111);
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
            assert_eq!(sender.remote_addr(), node1_addr);
            (sender, recv)
        }
        _ => {
            panic!("Need incoming")
        }
    };

    let (tran1_sender, mut tran1_recv) = match tran1.recv().await.unwrap() {
        TransportEvent::Outgoing(sender, recv, local_uuid) => {
            assert_eq!(sender.remote_node_id(), 2);
            assert_eq!(sender.remote_addr(), node2_addr);
            assert_eq!(local_uuid, 1111);
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

pub async fn simple_network_connect_addr_not_found<T: Transport>(mut tran1: T, dest: NodeAddr) {
    let connector1 = tran1.connector();
    connector1.connect_to(1111, 2, dest).unwrap();

    match tran1.recv().await.unwrap() {
        TransportEvent::OutgoingRequest(_node, _conn, acceptor, local_uuid) => {
            assert_eq!(local_uuid, 1111);
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

pub async fn simple_network_connect_wrong_node<T: Transport + Send + 'static>(mut tran1: T, _node1_addr: NodeAddr, mut tran2: T, node2_addr: NodeAddr) {
    let connector1 = tran1.connector();
    connector1.connect_to(1111, 3, node2_addr.clone()).unwrap();

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
        TransportEvent::OutgoingRequest(_node, _conn, acceptor, local_uuid) => {
            assert_eq!(local_uuid, 1111);
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

    join.cancel().await.print_none("Should cancel join");
}

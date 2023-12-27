use async_std::prelude::FutureExt;
use atm0s_sdn_identity::NodeAddr;
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::option_handle::OptionUtils;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

use crate::{
    msg::{MsgHeader, TransportMsg},
    transport::{ConnectionEvent, OutgoingConnectionError, Transport, TransportEvent},
};

#[derive(PartialEq, Debug, Serialize, Deserialize)]
enum Msg {
    Ping,
    Pong,
}

fn create_msg(msg: Msg, secure: bool) -> TransportMsg {
    let header = MsgHeader::build(0, 0, RouteRule::Direct).set_secure(secure).set_meta(0).set_stream_id(0);
    TransportMsg::build_raw(header, &bincode::serialize(&msg).unwrap())
}

pub async fn simple_network<T: Transport>(mut tran1: T, node1_addr: NodeAddr, mut tran2: T, node2_addr: NodeAddr) {
    let connector1 = tran1.connector();
    for conn in connector1.create_pending_outgoing(node2_addr.clone()) {
        connector1.continue_pending_outgoing(conn);
    }

    match tran2.recv().timeout(Duration::from_secs(2)).await.unwrap().unwrap() {
        TransportEvent::IncomingRequest(node, _conn, acceptor) => {
            log::info!("on incoming request");
            assert_eq!(node, 1);
            acceptor.accept();
        }
        _ => {
            panic!("Need IncomingRequest")
        }
    }

    let (tran2_sender, mut tran2_recv) = match tran2.recv().timeout(Duration::from_secs(2)).await.unwrap().unwrap() {
        TransportEvent::Incoming(sender, recv) => {
            assert_eq!(sender.remote_node_id(), 1);
            assert_eq!(sender.remote_addr(), node1_addr);
            (sender, recv)
        }
        _ => {
            panic!("Need incoming")
        }
    };

    let (tran1_sender, mut tran1_recv) = match tran1.recv().timeout(Duration::from_secs(2)).await.unwrap().unwrap() {
        TransportEvent::Outgoing(sender, recv) => {
            assert_eq!(sender.remote_node_id(), 2);
            assert_eq!(sender.remote_addr(), node2_addr);
            (sender, recv)
        }
        _ => {
            panic!("Need outgoing")
        }
    };

    let recv2_received = Arc::new(Mutex::new(vec![]));
    let recv2_received_clone = recv2_received.clone();
    async_std::task::spawn(async move {
        match tran2_recv.poll().timeout(Duration::from_secs(2)).await.unwrap().unwrap() {
            ConnectionEvent::Stats(_stats) => {}
            e => panic!("Should received stats {:?}", e),
        }
        tran2_sender.send(create_msg(Msg::Ping, true));
        tran2_sender.send(create_msg(Msg::Ping, false));
        let received_event = tran2_recv.poll().timeout(Duration::from_secs(2)).await.unwrap().unwrap();
        recv2_received_clone.lock().push(received_event);
        let received_event = tran2_recv.poll().timeout(Duration::from_secs(2)).await.unwrap().unwrap();
        recv2_received_clone.lock().push(received_event);
        assert_eq!(tran2_recv.poll().timeout(Duration::from_secs(2)).await.unwrap(), Err(()));
    });

    match tran1_recv.poll().timeout(Duration::from_secs(2)).await.unwrap().unwrap() {
        ConnectionEvent::Stats(_stats) => {}
        e => panic!("Should received stats {:?}", e),
    }

    let received_event = tran1_recv.poll().timeout(Duration::from_secs(2)).await.unwrap().unwrap();
    assert_eq!(received_event, ConnectionEvent::Msg(create_msg(Msg::Ping, true)));
    let received_event = tran1_recv.poll().timeout(Duration::from_secs(2)).await.unwrap().unwrap();
    assert_eq!(received_event, ConnectionEvent::Msg(create_msg(Msg::Ping, false)));

    tran1_sender.send(create_msg(Msg::Ping, true));
    tran1_sender.send(create_msg(Msg::Ping, false));
    async_std::task::sleep(Duration::from_millis(100)).await;

    assert_eq!(recv2_received.lock().len(), 2);
    assert_eq!(recv2_received.lock()[0], ConnectionEvent::Msg(create_msg(Msg::Ping, true)));
    assert_eq!(recv2_received.lock()[1], ConnectionEvent::Msg(create_msg(Msg::Ping, false)));

    tran1_sender.close();
    assert_eq!(tran1_recv.poll().timeout(Duration::from_secs(2)).await.unwrap(), Err(()));
}

pub async fn simple_network_connect_addr_not_found<T: Transport>(mut tran1: T, dest: NodeAddr) {
    let connector1 = tran1.connector();
    for conn in connector1.create_pending_outgoing(dest) {
        connector1.continue_pending_outgoing(conn);
    }

    match tran1.recv().timeout(Duration::from_secs(2)).await.unwrap().unwrap() {
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
    for conn in connector1.create_pending_outgoing(node2_addr.clone()) {
        connector1.continue_pending_outgoing(conn);
    }

    let join = async_std::task::spawn(async move {
        loop {
            match tran2.recv().timeout(Duration::from_secs(2)).await.unwrap() {
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

    match tran1.recv().timeout(Duration::from_secs(2)).await.unwrap().unwrap() {
        TransportEvent::OutgoingError { err, .. } => {
            assert_eq!(err, OutgoingConnectionError::AuthenticationError);
        }
        _ => {
            panic!("Need OutgoingError")
        }
    };

    join.cancel().await.print_none("Should cancel join");
}

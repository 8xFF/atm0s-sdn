use std::{net::SocketAddr, str::FromStr, time::Duration};

use crate::msg::{build_control_msg, HandshakeResult, UdpTransportMsg};
use async_std::{
    channel::{Receiver, Sender},
    net::UdpSocket,
    stream::StreamExt,
};
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use futures_util::{select, FutureExt};
use network::transport::{AsyncConnectionAcceptor, TransportEvent};
use utils::error_handle::ErrorUtils;

/// Connection handshake flow
/// Client -> Server: ConnectRequest
///                     - Client(retry each 1s until receive ConnectResponse or Timeout)
/// Server -> Client: ConnectResponse
///                     - Server(retry each 1s until receive ConnectResponseAck or Timeout)
///                     - Client => Done
/// Client -> Server: ConnectResponseAck
///                     - Server => Done
///

#[derive(Debug)]
pub enum IncomingHandshakeError {
    SocketError,
    Timeout,
    WrongMsg,
    InternalError,
    Rejected,
}

#[derive(Debug)]
pub enum OutgoingHandshakeError {
    DestinationError,
    SocketError,
    AuthenticationError,
    Timeout,
    WrongMsg,
    InternalError,
    Rejected,
}

pub async fn incoming_handshake(
    local_node_id: NodeId,
    internal_tx: &Sender<TransportEvent>,
    rx: &Receiver<([u8; 1500], usize)>,
    conn_id: ConnId,
    remote_addr: SocketAddr,
    socket: &UdpSocket,
) -> Result<(NodeId, NodeAddr), IncomingHandshakeError> {
    let mut count = 0;
    let mut result = None;
    let mut timer = async_std::stream::interval(Duration::from_secs(1));
    let mut requested = false;
    loop {
        select! {
            _ = timer.next().fuse() => {
                count += 1;
                if count >= 5 {
                    return Err(IncomingHandshakeError::Timeout);
                }

                if result.is_some() {
                    socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::Success)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                }
            }
            e = rx.recv().fuse() =>  match e {
                Ok((msg, size)) => {
                    if size == 0 || size >= 1500 || msg[0] != 255 {
                        return Err(IncomingHandshakeError::WrongMsg);
                    }

                    let msg = bincode::deserialize::<UdpTransportMsg>(&msg[1..]).map_err(|_| IncomingHandshakeError::WrongMsg)?;
                    match msg {
                        UdpTransportMsg::ConnectRequest(node, addr, to_node) => {
                            if to_node != local_node_id {
                                log::warn!("[IncomingHandshake] received from {} {} but wrong dest {} vs {}", node, addr, to_node, local_node_id);
                                socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::DestinationError)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                                return Err(IncomingHandshakeError::Rejected);
                            }
                            log::info!("[IncomingHandshake] received from {} {}", node, addr);
                            if !requested {
                                let (connection_acceptor, recv) = AsyncConnectionAcceptor::new();
                                internal_tx
                                    .send(TransportEvent::IncomingRequest(node, conn_id, connection_acceptor))
                                    .await
                                    .map_err(|_| IncomingHandshakeError::InternalError)?;
                                if let Err(e) = recv.recv().await.map_err(|_| IncomingHandshakeError::InternalError)? {
                                    log::warn!("[IncomingHandshake] {} {} handshake rejected {:?}", node, addr, e);
                                    socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::Rejected)), remote_addr).await.print_error("Should send handshake response error: Rejected");
                                    return Err(IncomingHandshakeError::Rejected);
                                }
                            }

                            requested = true;
                            if let Ok(addr) = NodeAddr::from_str(&addr) {
                                log::warn!("[IncomingHandshake] {} {} handshake success", node, addr);
                                socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::Success)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                                result = Some((node, addr));
                            } else {
                                log::warn!("[IncomingHandshake] {} {} handshake rejected wrong addr", node, addr);
                                socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::DestinationError)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                                return Err(IncomingHandshakeError::Rejected);
                            }
                        }
                        UdpTransportMsg::ConnectResponseAck(success) => {
                            if success {
                                log::info!("[IncomingHandshake] received handshake resonse ack {}", success);
                                if let Some(result) = result.take() {
                                    return Ok(result)
                                }
                            } else {
                                log::warn!("[IncomingHandshake] received handshake resonse ack {}", success);
                                return Err(IncomingHandshakeError::Rejected);
                            }
                        }
                        _ => {}
                    };
                },
                Err(e) => {
                    log::warn!("[IncomingHandshake] internal queue error {:?}", e);
                    return Err(IncomingHandshakeError::InternalError);
                }
            }
        }
    }
}

pub async fn outgoing_handshake(socket: &UdpSocket, local_node_id: NodeId, local_node_addr: NodeAddr, to_node_id: NodeId) -> Result<(), OutgoingHandshakeError> {
    let mut timer = async_std::stream::interval(Duration::from_secs(1));
    let mut buf = [0; 1500];
    let mut count = 0;

    socket
        .send(&build_control_msg(&UdpTransportMsg::ConnectRequest(local_node_id, local_node_addr.to_string(), to_node_id)))
        .await
        .map_err(|_| OutgoingHandshakeError::SocketError)?;

    log::info!("[OutgoingHandshake {}] send handshake connect request", to_node_id);

    loop {
        select! {
            _ = timer.next().fuse() => {
                count += 1;
                if count >= 5 {
                    return Err(OutgoingHandshakeError::Timeout);
                }
                log::warn!("[OutgoingHandshake {}] send handshake connect request (retry {})", to_node_id, count);
                socket.send(&build_control_msg(&UdpTransportMsg::ConnectRequest(local_node_id, local_node_addr.to_string(), to_node_id))).await.map_err(|_| OutgoingHandshakeError::SocketError)?;
            }
            e = socket.recv_from(&mut buf).fuse() => match e {
                Ok((size, _remote_addr)) => {
                    if size == 0 || size >= 1500 || buf[0] != 255 {
                        return Err(OutgoingHandshakeError::WrongMsg);
                    }

                    let msg = bincode::deserialize::<UdpTransportMsg>(&buf[1..size]).map_err(|_| OutgoingHandshakeError::WrongMsg)?;
                    match msg {
                        UdpTransportMsg::ConnectResponse(res) => {
                            log::info!("[OutgoingHandshake {}] received handshake response {:?}", to_node_id, res);
                            match res {
                                HandshakeResult::Success => {
                                    socket.send(&build_control_msg(&UdpTransportMsg::ConnectResponseAck(true))).await.map_err(|_| OutgoingHandshakeError::SocketError)?;
                                    return Ok(());
                                }
                                HandshakeResult::DestinationError => {
                                    return Err(OutgoingHandshakeError::DestinationError);
                                }
                                HandshakeResult::AuthenticationError => {
                                    return Err(OutgoingHandshakeError::AuthenticationError);
                                }
                                HandshakeResult::Rejected => {
                                    return Err(OutgoingHandshakeError::Rejected);
                                }
                            }
                        }
                        _ => {}
                    };
                }
                Err(e) => {
                    log::warn!("[IncomingHandshake] socket error {:?}", e);
                    return Err(OutgoingHandshakeError::SocketError);
                }
            }
        }
    }
}

use std::{net::SocketAddr, ops::Deref, sync::Arc, time::Duration};

use crate::msg::{build_control_msg, HandshakeRequest, HandshakeResult, UdpTransportMsg};
use async_std::{
    channel::{Receiver, Sender},
    net::UdpSocket,
    stream::StreamExt,
};
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_network::{
    secure::{DataSecure, ObjectSecure},
    transport::{AsyncConnectionAcceptor, TransportEvent},
};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use futures_util::{select, FutureExt};

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
    SocketError,
    AuthenticationError,
    Timeout,
    WrongMsg,
    Rejected,
}

pub async fn incoming_handshake(
    secure: Arc<dyn DataSecure>,
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

                if let Some((remote_node_id, _)) = &result {
                    let signature = ObjectSecure::sign_obj(secure.deref(), *remote_node_id, &HandshakeResult::Success);
                    socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::Success, signature)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                }
            }
            e = rx.recv().fuse() =>  match e {
                Ok((msg, size)) => {
                    if size == 0 || size >= 1500 || msg[0] != 255 {
                        return Err(IncomingHandshakeError::WrongMsg);
                    }

                    let msg = bincode::deserialize::<UdpTransportMsg>(&msg[1..]).map_err(|_| IncomingHandshakeError::WrongMsg)?;
                    match msg {
                        UdpTransportMsg::ConnectRequest(req, sig) => {
                            if !ObjectSecure::verify_obj(secure.deref(), req.remote_node_id, &req, &sig) {
                                log::warn!("[UdpTransport] received handshake request wrong signature");
                                let signature = ObjectSecure::sign_obj(secure.deref(), req.node_id, &HandshakeResult::AuthenticationError);
                                socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::AuthenticationError, signature)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                                return Err(IncomingHandshakeError::Rejected);
                            }
                            if req.remote_node_id != local_node_id {
                                log::warn!("[UdpTransport] received from {} {} but wrong dest {} vs {}", req.node_id, req.node_addr, req.remote_node_id, local_node_id);
                                let signature = ObjectSecure::sign_obj(secure.deref(), req.node_id, &HandshakeResult::AuthenticationError);
                                socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::AuthenticationError, signature)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                                return Err(IncomingHandshakeError::Rejected);
                            }
                            log::info!("[UdpTransport] received from {} {}", req.node_id, req.node_addr);
                            if !requested {
                                let (connection_acceptor, recv) = AsyncConnectionAcceptor::new();
                                internal_tx
                                    .send(TransportEvent::IncomingRequest(req.node_id, conn_id, connection_acceptor))
                                    .await
                                    .map_err(|_| IncomingHandshakeError::InternalError)?;
                                if let Err(e) = recv.recv().await.map_err(|_| IncomingHandshakeError::InternalError)? {
                                    log::warn!("[UdpTransport] {} {} handshake rejected {:?}", req.node_id, req.node_addr, e);
                                    let signature = ObjectSecure::sign_obj(secure.deref(), req.node_id, &HandshakeResult::Rejected);
                                    socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::Rejected, signature)), remote_addr).await.print_error("Should send handshake response error: Rejected");
                                    return Err(IncomingHandshakeError::Rejected);
                                }
                            }

                            requested = true;
                            log::info!("[UdpTransport] {} {} handshake success", req.node_id, req.node_addr);
                            let signature = ObjectSecure::sign_obj(secure.deref(), req.node_id, &HandshakeResult::Success);
                            socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(HandshakeResult::Success, signature)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                            result = Some((req.node_id, req.node_addr));
                        }
                        UdpTransportMsg::ConnectResponseAck(success) => {
                            if success {
                                log::info!("[UdpTransport] received handshake resonse ack {}", success);
                                if let Some(result) = result.take() {
                                    return Ok(result)
                                }
                            } else {
                                log::warn!("[UdpTransport] received handshake resonse ack {}", success);
                                return Err(IncomingHandshakeError::Rejected);
                            }
                        }
                        _ => {}
                    };
                },
                Err(e) => {
                    log::warn!("[UdpTransport] internal queue error {:?}", e);
                    return Err(IncomingHandshakeError::InternalError);
                }
            }
        }
    }
}

pub async fn outgoing_handshake(secure: Arc<dyn DataSecure>, socket: &UdpSocket, local_node_id: NodeId, local_node_addr: NodeAddr, to_node_id: NodeId) -> Result<(), OutgoingHandshakeError> {
    let mut timer = async_std::stream::interval(Duration::from_secs(1));
    let mut buf = [0; 1500];
    let mut count = 0;

    let req = HandshakeRequest {
        node_id: local_node_id,
        node_addr: local_node_addr.clone(),
        remote_node_id: to_node_id,
    };
    let signature = ObjectSecure::sign_obj(secure.deref(), to_node_id, &req);

    socket
        .send(&build_control_msg(&UdpTransportMsg::ConnectRequest(req.clone(), signature.clone())))
        .await
        .map_err(|_| OutgoingHandshakeError::SocketError)?;

    log::info!("[UdpTransport] send handshake connect request");

    loop {
        select! {
            _ = timer.next().fuse() => {
                count += 1;
                if count >= 5 {
                    return Err(OutgoingHandshakeError::Timeout);
                }
                log::warn!("[UdpTransport] send handshake connect request (retry {})", count);
                socket.send(&build_control_msg(&UdpTransportMsg::ConnectRequest(req.clone(), signature.clone()))).await.map_err(|_| OutgoingHandshakeError::SocketError)?;
            }
            e = socket.recv_from(&mut buf).fuse() => match e {
                Ok((size, _remote_addr)) => {
                    if size == 0 || size >= 1500 || buf[0] != 255 {
                        return Err(OutgoingHandshakeError::WrongMsg);
                    }

                    let msg = bincode::deserialize::<UdpTransportMsg>(&buf[1..size]).map_err(|_| OutgoingHandshakeError::WrongMsg)?;
                    match msg {
                        UdpTransportMsg::ConnectResponse(res, signature) => {
                            if !ObjectSecure::verify_obj(secure.deref(), local_node_id, &res, &signature) {
                                log::warn!("[UdpTransport] received handshake response wrong signature");
                                return Err(OutgoingHandshakeError::AuthenticationError);
                            }
                            log::info!("[UdpTransport] received handshake response {:?}", res);
                            match res {
                                HandshakeResult::Success => {
                                    socket.send(&build_control_msg(&UdpTransportMsg::ConnectResponseAck(true))).await.map_err(|_| OutgoingHandshakeError::SocketError)?;
                                    return Ok(());
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
                    log::warn!("[UdpTransport] socket error {:?}", e);
                    return Err(OutgoingHandshakeError::SocketError);
                }
            }
        }
    }
}

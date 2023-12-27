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
use snow::TransportState;

static SNOW_PATTERN: &'static str = "Noise_NN_25519_ChaChaPoly_BLAKE2s";

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
) -> Result<(NodeId, NodeAddr, TransportState), IncomingHandshakeError> {
    let mut count = 0;
    let mut result: Option<(u32, NodeAddr, Vec<u8>)> = None;
    let mut timer = async_std::stream::interval(Duration::from_secs(1));
    let mut requested = false;
    let mut snow_buf = [0; 1500];
    let mut snow_responder = snow::Builder::new(SNOW_PATTERN.parse().expect("")).build_responder().expect("");

    loop {
        select! {
            _ = timer.next().fuse() => {
                count += 1;
                if count >= 5 {
                    return Err(IncomingHandshakeError::Timeout);
                }

                if let Some((remote_node_id, _, snow_res)) = &result {
                    let handshake_res = HandshakeResult::Success(snow_res.clone());
                    let signature = ObjectSecure::sign_obj(secure.deref(), *remote_node_id, &handshake_res);
                    socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(handshake_res, signature)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
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
                            match snow_responder.read_message(&req.snow_handshake, &mut snow_buf) {
                                Ok(_) => {
                                    match snow_responder.write_message(&[], &mut snow_buf) {
                                        Ok(len) => {
                                            let handshake_res = HandshakeResult::Success(snow_buf[..len].to_vec());
                                            log::info!("[UdpTransport] {} {} handshake success", req.node_id, req.node_addr);
                                            let signature = ObjectSecure::sign_obj(secure.deref(), req.node_id, &handshake_res);
                                            socket.send_to(&build_control_msg(&UdpTransportMsg::ConnectResponse(handshake_res, signature)), remote_addr).await.map_err(|_| IncomingHandshakeError::SocketError)?;
                                            result = Some((req.node_id, req.node_addr, snow_buf[..len].to_vec(),));
                                        },
                                        Err(e) => {
                                            log::error!("[UdpTransport] received hanshake snow write message error {:?}", e);
                                            return Err(IncomingHandshakeError::InternalError);
                                        }
                                    }
                                },
                                Err(e) => {
                                    log::error!("[UdpTransport] received hanshake snow read message error {:?}", e);
                                    return Err(IncomingHandshakeError::WrongMsg);
                                }
                            }
                        }
                        UdpTransportMsg::ConnectResponseAck(success) => {
                            if success {
                                log::info!("[UdpTransport] received handshake resonse ack {}", success);
                                if let Some((node, addr, _)) = result.take() {
                                    return Ok((node, addr, snow_responder.into_transport_mode().expect("Should be transport mode")));
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

pub async fn outgoing_handshake(
    secure: Arc<dyn DataSecure>,
    socket: &UdpSocket,
    local_node_id: NodeId,
    local_node_addr: NodeAddr,
    to_node_id: NodeId,
) -> Result<TransportState, OutgoingHandshakeError> {
    let mut timer = async_std::stream::interval(Duration::from_secs(1));
    let mut buf = [0; 1500];
    let mut count = 0;

    let mut snow_initiator = snow::Builder::new(SNOW_PATTERN.parse().expect("")).build_initiator().expect("");
    let snow_hanshake_len = snow_initiator.write_message(&[], &mut buf).expect("");

    let req = HandshakeRequest {
        node_id: local_node_id,
        node_addr: local_node_addr.clone(),
        remote_node_id: to_node_id,
        snow_handshake: buf[..snow_hanshake_len].to_vec(),
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
                                HandshakeResult::Success(snow_response) => {
                                    match snow_initiator.read_message(&snow_response, &mut buf) {
                                        Ok(_) => {
                                            match snow_initiator.into_transport_mode() {
                                                Ok(state) => {
                                                    socket.send(&build_control_msg(&UdpTransportMsg::ConnectResponseAck(true))).await.map_err(|_| OutgoingHandshakeError::SocketError)?;
                                                    return Ok(state)
                                                },
                                                Err(e) => {
                                                    log::error!("[UdpTransport] received hanshake snow into_transport_mode error {:?}", e);
                                                    return Err(OutgoingHandshakeError::AuthenticationError);
                                                }
                                            }
                                        },
                                        Err(e) => {
                                            log::error!("[UdpTransport] received hanshake snow read message error {:?}", e);
                                            return Err(OutgoingHandshakeError::AuthenticationError);
                                        }
                                    }
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

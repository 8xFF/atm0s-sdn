use crate::connection::{recv_tcp_stream, send_tcp_stream, AsyncBincodeStreamU16};
use crate::msg::{HandshakeRequest, HandshakeResult, TcpMsg};
use async_std::channel::Sender;
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_network::secure::{DataSecure, ObjectSecure};
use atm0s_sdn_network::transport::{AsyncConnectionAcceptor, TransportEvent};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use snow::TransportState;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

static SNOW_PATTERN: &str = "Noise_NN_25519_ChaChaPoly_BLAKE2s";

pub enum IncomingHandshakeError {
    SocketError,
    Timeout,
    WrongMsg,
    InternalError,
    Rejected,
    ValidateError,
}

pub async fn incoming_handshake(
    secure: Arc<dyn DataSecure>,
    my_node: NodeId,
    socket: &mut AsyncBincodeStreamU16,
    conn_id: ConnId,
    internal_tx: &Sender<TransportEvent>,
) -> Result<(NodeId, NodeAddr, TransportState), IncomingHandshakeError> {
    log::info!("[TcpTransport] handshake wait ConnectRequest");

    let mut snow_buf = [0; 1500];
    let mut snow_responder = snow::Builder::new(SNOW_PATTERN.parse().expect("")).build_responder().expect("");

    let msg = async_std::future::timeout(Duration::from_secs(5), recv_tcp_stream(socket))
        .await
        .map_err(|_| IncomingHandshakeError::Timeout)?
        .map_err(|_| IncomingHandshakeError::SocketError)?;
    let (remote_node, remote_addr, snow_handshake) = match msg {
        TcpMsg::ConnectRequest(req, sig) => {
            if req.remote_node_id == my_node && ObjectSecure::verify_obj(secure.deref(), req.remote_node_id, &req, &sig) {
                log::info!("[TcpTransport] handshake from {} {}", req.node_id, req.node_addr);
                (req.node_id, req.node_addr, req.snow_handshake)
            } else {
                log::warn!("[TcpTransport] handshake from wrong node info {} vs {}", req.remote_node_id, my_node);
                let sig = ObjectSecure::sign_obj(secure.deref(), req.node_id, &HandshakeResult::AuthenticationError);
                send_tcp_stream(socket, TcpMsg::ConnectResponse(HandshakeResult::AuthenticationError, sig))
                    .await
                    .print_error("Should send handshake response error: Wrong node");
                return Err(IncomingHandshakeError::ValidateError);
            }
        }
        _ => {
            log::warn!("[TcpTransport] handshake wrong msg");
            return Err(IncomingHandshakeError::WrongMsg);
        }
    };

    let (connection_acceptor, recv) = AsyncConnectionAcceptor::new();
    internal_tx
        .send(TransportEvent::IncomingRequest(remote_node, conn_id, connection_acceptor))
        .await
        .map_err(|_| IncomingHandshakeError::InternalError)?;
    if let Err(e) = recv.recv().await.map_err(|_| IncomingHandshakeError::InternalError)? {
        log::error!("handshake rejected {:?}", e);
        let sig = ObjectSecure::sign_obj(secure.deref(), remote_node, &HandshakeResult::Rejected);
        send_tcp_stream(socket, TcpMsg::ConnectResponse(HandshakeResult::Rejected, sig))
            .await
            .print_error("Should send handshake response error: Rejected");
        return Err(IncomingHandshakeError::Rejected);
    }

    let handshake_res = match snow_responder.read_message(&snow_handshake, &mut snow_buf) {
        Ok(_) => match snow_responder.write_message(&[], &mut snow_buf) {
            Ok(snow_len) => HandshakeResult::Success(snow_buf[..snow_len].to_vec()),
            Err(e) => {
                log::error!("[TcpTransport] handshake snow write error {:?}", e);
                HandshakeResult::AuthenticationError
            }
        },
        Err(e) => {
            log::error!("[TcpTransport] handshake snow read error {:?}", e);
            HandshakeResult::AuthenticationError
        }
    };
    let sig = ObjectSecure::sign_obj(secure.deref(), remote_node, &handshake_res);
    send_tcp_stream(socket, TcpMsg::ConnectResponse(handshake_res, sig))
        .await
        .print_error("Should send handshake response error: Ok");

    Ok((remote_node, remote_addr, snow_responder.into_transport_mode().expect("Should be transport mode")))
}

#[derive(Debug)]
pub enum OutgoingHandshakeError {
    SocketError,
    Timeout,
    WrongMsg,
    Rejected,
    AuthenticationError,
}

pub async fn outgoing_handshake(
    secure: Arc<dyn DataSecure>,
    remote_node: NodeId,
    my_node: NodeId,
    my_node_addr: NodeAddr,
    socket: &mut AsyncBincodeStreamU16,
    _conn_id: ConnId,
    _internal_tx: &Sender<TransportEvent>,
) -> Result<TransportState, OutgoingHandshakeError> {
    log::info!("[TcpTransport] outgoing_handshake send ConnectRequest to {}", remote_node);
    let mut buf = [0; 1500];
    let mut snow_initiator = snow::Builder::new(SNOW_PATTERN.parse().expect("")).build_initiator().expect("");
    let snow_hanshake_len = snow_initiator.write_message(&[], &mut buf).expect("");

    let req = HandshakeRequest {
        node_id: my_node,
        node_addr: my_node_addr,
        remote_node_id: remote_node,
        snow_handshake: buf[..snow_hanshake_len].to_vec(),
    };
    let sig = ObjectSecure::sign_obj(secure.deref(), remote_node, &req);
    send_tcp_stream(socket, TcpMsg::ConnectRequest(req, sig)).await.map_err(|_| OutgoingHandshakeError::SocketError)?;

    log::info!("[TcpTransport] outgoing_handshake wait ConnectResponse from {}", remote_node);
    let msg = async_std::future::timeout(Duration::from_secs(5), recv_tcp_stream(socket))
        .await
        .map_err(|_| {
            log::info!("[TcpTransport] outgoing_handshake wait ConnectResponse from {} timeout", remote_node);
            OutgoingHandshakeError::Timeout
        })?
        .map_err(|_| OutgoingHandshakeError::SocketError)?;
    match msg {
        TcpMsg::ConnectResponse(res, sig) => {
            if ObjectSecure::verify_obj(secure.deref(), my_node, &res, &sig) {
                match res {
                    HandshakeResult::Success(snow_response) => {
                        log::info!("[TcpTransport] outgoing_handshake ConnectResponse from {} success", remote_node);
                        match snow_initiator.read_message(&snow_response, &mut buf) {
                            Ok(_) => match snow_initiator.into_transport_mode() {
                                Ok(state) => Ok(state),
                                Err(e) => {
                                    log::error!("[TcpTransport] received hanshake snow into_transport_mode error {:?}", e);
                                    Err(OutgoingHandshakeError::AuthenticationError)
                                }
                            },
                            Err(e) => {
                                log::error!("[TcpTransport] received hanshake snow read message error {:?}", e);
                                Err(OutgoingHandshakeError::AuthenticationError)
                            }
                        }
                    }
                    HandshakeResult::Rejected => {
                        log::warn!("[TcpTransport] outgoing_handshake ConnectResponse from {} rejected", remote_node);
                        Err(OutgoingHandshakeError::Rejected)
                    }
                    HandshakeResult::AuthenticationError => {
                        log::warn!("[TcpTransport] outgoing_handshake ConnectResponse from {} auth error", remote_node);
                        Err(OutgoingHandshakeError::WrongMsg)
                    }
                }
            } else {
                log::warn!("[TcpTransport] outgoing_handshake ConnectResponse from {} wrong sig", remote_node);
                Err(OutgoingHandshakeError::WrongMsg)
            }
        }
        _ => Err(OutgoingHandshakeError::WrongMsg),
    }
}

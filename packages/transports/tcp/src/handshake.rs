use crate::connection::{recv_tcp_stream, send_tcp_stream, AsyncBincodeStreamU16};
use crate::msg::{HandshakeRequest, HandshakeResult, TcpMsg};
use async_std::channel::Sender;
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_network::secure::{DataSecure, ObjectSecure};
use atm0s_sdn_network::transport::{AsyncConnectionAcceptor, TransportEvent};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

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
) -> Result<(NodeId, NodeAddr), IncomingHandshakeError> {
    log::info!("[TcpTransport] handshake wait ConnectRequest");

    let msg = async_std::future::timeout(Duration::from_secs(5), recv_tcp_stream(socket))
        .await
        .map_err(|_| IncomingHandshakeError::Timeout)?
        .map_err(|_| IncomingHandshakeError::SocketError)?;
    let (remote_node, remote_addr) = match msg {
        TcpMsg::ConnectRequest(req, sig) => {
            if req.remote_node_id == my_node && ObjectSecure::verify_obj(secure.deref(), req.remote_node_id, &req, &sig) {
                log::info!("[TcpTransport] handshake from {} {}", req.node_id, req.node_addr);
                (req.node_id, req.node_addr)
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

    let sig = ObjectSecure::sign_obj(secure.deref(), remote_node, &HandshakeResult::Success);
    send_tcp_stream(socket, TcpMsg::ConnectResponse(HandshakeResult::Success, sig))
        .await
        .print_error("Should send handshake response error: Ok");

    Ok((remote_node, remote_addr))
}

#[derive(Debug)]
pub enum OutgoingHandshakeError {
    SocketError,
    Timeout,
    WrongMsg,
    Rejected,
}

pub async fn outgoing_handshake(
    secure: Arc<dyn DataSecure>,
    remote_node: NodeId,
    my_node: NodeId,
    my_node_addr: NodeAddr,
    socket: &mut AsyncBincodeStreamU16,
    _conn_id: ConnId,
    _internal_tx: &Sender<TransportEvent>,
) -> Result<(), OutgoingHandshakeError> {
    log::info!("[TcpTransport] outgoing_handshake send ConnectRequest to {}", remote_node);
    let req = HandshakeRequest {
        node_id: my_node,
        node_addr: my_node_addr,
        remote_node_id: remote_node,
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
                    HandshakeResult::Success => {
                        log::info!("[TcpTransport] outgoing_handshake ConnectResponse from {} success", remote_node);
                    }
                    HandshakeResult::Rejected => {
                        log::warn!("[TcpTransport] outgoing_handshake ConnectResponse from {} rejected", remote_node);
                        return Err(OutgoingHandshakeError::Rejected);
                    }
                    HandshakeResult::AuthenticationError => {
                        log::warn!("[TcpTransport] outgoing_handshake ConnectResponse from {} auth error", remote_node);
                        return Err(OutgoingHandshakeError::WrongMsg);
                    }
                }
            } else {
                log::warn!("[TcpTransport] outgoing_handshake ConnectResponse from {} wrong sig", remote_node);
                return Err(OutgoingHandshakeError::WrongMsg);
            }
        }
        _ => return Err(OutgoingHandshakeError::WrongMsg),
    };
    Ok(())
}

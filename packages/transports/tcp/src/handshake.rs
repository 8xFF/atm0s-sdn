use crate::connection::{recv_tcp_stream, send_tcp_stream, BUFFER_LEN};
use crate::msg::TcpMsg;
use async_std::channel::{RecvError, Sender};
use async_std::net::TcpStream;
use bluesea_identity::{PeerAddr, PeerId};
use futures_util::io::{ReadHalf, WriteHalf};
use futures_util::AsyncWriteExt;
use network::transport::{
    AsyncConnectionAcceptor, ConnectionRejectReason, OutgoingConnectionError, TransportEvent,
};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

pub enum IncomingHandshakeError {
    SocketError,
    Timeout,
    WrongMsg,
    InternalError,
    Rejected,
    NetError,
    ValidateError,
}

pub async fn incoming_handshake<MSG: Serialize + DeserializeOwned>(
    my_peer: PeerId,
    my_addr: PeerAddr,
    socket: &mut TcpStream,
    conn_id: u32,
    internal_tx: &Sender<TransportEvent<MSG>>,
) -> Result<(PeerId, PeerAddr), IncomingHandshakeError> {
    log::info!("[TcpTransport] handshake wait ConnectRequest");

    let mut buf = [0; BUFFER_LEN];
    let msg = async_std::future::timeout(
        Duration::from_secs(5),
        recv_tcp_stream::<MSG>(&mut buf, socket),
    )
    .await
    .map_err(|_| IncomingHandshakeError::Timeout)?
    .map_err(|_| IncomingHandshakeError::SocketError)?;
    let (remote_peer, remote_addr) = match msg {
        TcpMsg::ConnectRequest(my_peer_2, peer, addr) => {
            if my_peer_2 == my_peer {
                log::info!("[TcpTransport] handshake from {} {}", peer, addr);
                (peer, addr)
            } else {
                log::warn!(
                    "[TcpTransport] handshake from wrong peer info {} vs {}",
                    my_peer_2,
                    my_peer
                );
                send_tcp_stream(
                    socket,
                    TcpMsg::<MSG>::ConnectResponse(Err("WrongPeer".to_string())),
                )
                .await;
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
        .send(TransportEvent::IncomingRequest(
            remote_peer,
            conn_id,
            connection_acceptor,
        ))
        .await
        .map_err(|_| IncomingHandshakeError::InternalError)?;
    if let Err(e) = recv
        .recv()
        .await
        .map_err(|_| IncomingHandshakeError::InternalError)?
    {
        send_tcp_stream(
            socket,
            TcpMsg::<MSG>::ConnectResponse(Err("Rejected".to_string())),
        )
        .await;
        return Err(IncomingHandshakeError::Rejected);
    }
    send_tcp_stream(
        socket,
        TcpMsg::<MSG>::ConnectResponse(Ok((my_peer, my_addr))),
    )
    .await;

    Ok((remote_peer, remote_addr))
}

pub enum OutgoingHandshakeError {
    SocketError,
    Timeout,
    WrongMsg,
    InternalError,
    Rejected,
    NetError,
}

pub async fn outgoing_handshake<MSG: Serialize + DeserializeOwned>(
    remote_peer: PeerId,
    my_peer: PeerId,
    my_peer_addr: PeerAddr,
    socket: &mut TcpStream,
    conn_id: u32,
    internal_tx: &Sender<TransportEvent<MSG>>,
) -> Result<(), OutgoingHandshakeError> {
    let mut buf = [0; BUFFER_LEN];

    log::info!(
        "[TcpTransport] outgoing_handshake send ConnectRequest to {}",
        remote_peer
    );
    send_tcp_stream(
        socket,
        TcpMsg::<MSG>::ConnectRequest(remote_peer, my_peer, my_peer_addr),
    )
    .await
    .map_err(|_| OutgoingHandshakeError::SocketError)?;

    log::info!(
        "[TcpTransport] outgoing_handshake wait ConnectResponse from {}",
        remote_peer
    );
    let msg = async_std::future::timeout(
        Duration::from_secs(5),
        recv_tcp_stream::<MSG>(&mut buf, socket),
    )
    .await
    .map_err(|_| {
        log::info!(
            "[TcpTransport] outgoing_handshake wait ConnectResponse from {} timeout",
            remote_peer
        );
        OutgoingHandshakeError::Timeout
    })?
    .map_err(|_| OutgoingHandshakeError::SocketError)?;
    let _peer = match msg {
        TcpMsg::ConnectResponse(Ok((peer_id, addr))) => {
            log::info!(
                "[TcpTransport] outgoing_handshake ConnectResponse Ok from {} {}",
                peer_id,
                addr
            );
            peer_id
        }
        TcpMsg::ConnectResponse(Err(err)) => {
            log::info!(
                "[TcpTransport] outgoing_handshake ConnectResponse Err {}",
                err
            );
            return Err(OutgoingHandshakeError::Rejected);
        }
        _ => return Err(OutgoingHandshakeError::WrongMsg),
    };
    Ok(())
}

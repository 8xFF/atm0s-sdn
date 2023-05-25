use crate::msg::TcpMsg;
use async_std::channel::{bounded, unbounded, Receiver, RecvError, Sender};
use async_std::net::{Shutdown, TcpStream};
use bluesea_identity::{PeerAddr, PeerId};
use futures_util::io::{ReadHalf, WriteHalf};
use futures_util::{select, FutureExt};
use futures_util::{AsyncReadExt, AsyncWriteExt};
use network::transport::{ConnectionEvent, ConnectionMsg, ConnectionReceiver, ConnectionSender};
use serde::{de::DeserializeOwned, Serialize};

pub const BUFFER_LEN: usize = 1500;

pub async fn send_tcp_stream<MSG: Serialize>(
    writer: &mut TcpStream,
    msg: TcpMsg<MSG>,
) -> Result<(), ()> {
    match bincode::serialize(&msg) {
        Ok(buf) => match writer.write_all(&buf).await {
            Ok(_) => Ok(()),
            Err(err) => {
                log::error!("[TcpTransport] write buffer error {:?}", err);
                Err(())
            }
        },
        Err(err) => {
            log::error!("[TcpTransport] serialize buffer error {:?}", err);
            Err(())
        }
    }
}

enum OutgoingEvent<MSG> {
    Msg(TcpMsg<MSG>),
    Close,
}

pub struct TcpConnectionSender<MSG> {
    remote_peer_id: PeerId,
    remote_addr: PeerAddr,
    conn_id: u32,
    reliable_sender: Sender<OutgoingEvent<MSG>>,
    unreliable_sender: Sender<OutgoingEvent<MSG>>,
}

impl<MSG> TcpConnectionSender<MSG>
where
    MSG: Serialize + Send + Sync + 'static,
{
    pub fn new(
        remote_peer_id: PeerId,
        remote_addr: PeerAddr,
        conn_id: u32,
        unreliable_queue_size: usize,
        mut socket: TcpStream,
    ) -> Self {
        let (reliable_sender, mut r_rx) = unbounded();
        let (unreliable_sender, mut unr_rx) = bounded(unreliable_queue_size);

        let task = async_std::task::spawn(async move {
            log::info!("[TcpConnectionSender] start sending loop");
            loop {
                let msg: Result<OutgoingEvent<MSG>, RecvError> = select! {
                    e = r_rx.recv().fuse() => e,
                    e = unr_rx.recv().fuse() => e,
                };

                match msg {
                    Ok(OutgoingEvent::Msg(msg)) => {
                        if let Err(e) = send_tcp_stream(&mut socket, msg).await {}
                    }
                    Ok(OutgoingEvent::Close) => {
                        if let Err(e) = socket.shutdown(Shutdown::Both) {
                            log::error!("[TcpTrasport] close sender error {}", e);
                        } else {
                            log::info!("[TcpTrasport] close sender loop");
                        }
                        break;
                    }
                    Err(err) => {
                        log::error!("[TcpTransport] channel error {:?}", err);
                        break;
                    }
                }
            }
            log::info!("[TcpConnectionSender] stop sending loop");
        });

        Self {
            remote_addr,
            remote_peer_id,
            conn_id,
            reliable_sender,
            unreliable_sender,
        }
    }
}

impl<MSG> ConnectionSender<MSG> for TcpConnectionSender<MSG>
where
    MSG: Send + Sync + 'static,
{
    fn remote_peer_id(&self) -> PeerId {
        self.remote_peer_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> PeerAddr {
        self.remote_addr.clone()
    }

    fn send(&self, service_id: u8, msg: ConnectionMsg<MSG>) {
        match &msg {
            ConnectionMsg::Reliable { .. } => {
                if let Err(e) = self
                    .reliable_sender
                    .send_blocking(OutgoingEvent::Msg(TcpMsg::Msg(service_id, msg)))
                {
                    log::error!("[ConnectionSender] send reliable msg error {:?}", e);
                }
            }
            ConnectionMsg::Unreliable { .. } => {
                if let Err(e) = self
                    .unreliable_sender
                    .try_send(OutgoingEvent::Msg(TcpMsg::Msg(service_id, msg)))
                {
                    log::error!("[ConnectionSender] send unreliable msg error {:?}", e);
                }
            }
        }
    }

    fn close(&self) {
        if let Err(e) = self.unreliable_sender.send_blocking(OutgoingEvent::Close) {
            log::error!("[ConnectionSender] send Close request error {:?}", e);
        } else {
            log::info!("[ConnectionSender] sent close request");
        }
    }
}

pub async fn recv_tcp_stream<MSG: DeserializeOwned>(
    buf: &mut [u8],
    reader: &mut TcpStream,
) -> Result<TcpMsg<MSG>, ()> {
    let size = reader.read(buf).await.map_err(|_| ())?;
    bincode::deserialize::<TcpMsg<MSG>>(&buf[0..size]).map_err(|_| ())
}

pub struct TcpConnectionReceiver {
    pub(crate) remote_peer_id: PeerId,
    pub(crate) remote_addr: PeerAddr,
    pub(crate) conn_id: u32,
    pub(crate) socket: TcpStream,
    pub(crate) buf: [u8; BUFFER_LEN],
}

#[async_trait::async_trait]
impl<MSG> ConnectionReceiver<MSG> for TcpConnectionReceiver
where
    MSG: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn remote_peer_id(&self) -> PeerId {
        self.remote_peer_id
    }

    fn connection_id(&self) -> u32 {
        self.conn_id
    }

    fn remote_addr(&self) -> PeerAddr {
        self.remote_addr.clone()
    }

    async fn poll(&mut self) -> Result<ConnectionEvent<MSG>, ()> {
        loop {
            match recv_tcp_stream::<MSG>(&mut self.buf, &mut self.socket).await {
                Ok(msg) => match msg {
                    TcpMsg::Msg(service_id, msg) => {
                        break Ok(ConnectionEvent::Msg { service_id, msg });
                    }
                    _ => {
                        log::warn!("[ConnectionReceiver] wrong msg type, required TcpMsg::Msg");
                    }
                },
                Err(e) => {
                    log::info!("[ConnectionReceiver] stream closed");
                    break Err(());
                }
            }
        }
    }
}

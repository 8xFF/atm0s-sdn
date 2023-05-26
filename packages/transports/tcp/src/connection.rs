use crate::msg::TcpMsg;
use async_std::channel::{bounded, unbounded, Receiver, RecvError, Sender};
use async_std::net::{Shutdown, TcpStream};
use async_std::task::JoinHandle;
use bluesea_identity::{PeerAddr, PeerId};
use futures_util::io::{ReadHalf, WriteHalf};
use futures_util::{select, AsyncReadExt, AsyncWriteExt, FutureExt, StreamExt};
use network::transport::{
    ConnectionEvent, ConnectionMsg, ConnectionReceiver, ConnectionSender, ConnectionStats,
};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use std::time::Duration;
use utils::Timer;

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

pub enum OutgoingEvent<MSG> {
    Msg(TcpMsg<MSG>),
    CloseRequest,
    ClosedNotify,
}

pub struct TcpConnectionSender<MSG> {
    remote_peer_id: PeerId,
    remote_addr: PeerAddr,
    conn_id: u32,
    reliable_sender: Sender<OutgoingEvent<MSG>>,
    unreliable_sender: Sender<OutgoingEvent<MSG>>,
    task: Option<JoinHandle<()>>,
}

impl<MSG> TcpConnectionSender<MSG>
where
    MSG: Serialize + Send + Sync + 'static,
{
    pub fn new(
        peer_id: PeerId,
        remote_peer_id: PeerId,
        remote_addr: PeerAddr,
        conn_id: u32,
        unreliable_queue_size: usize,
        mut socket: TcpStream,
        timer: Arc<dyn Timer>,
    ) -> (Self, Sender<OutgoingEvent<MSG>>) {
        let (reliable_sender, mut r_rx) = unbounded();
        let (unreliable_sender, mut unr_rx) = bounded(unreliable_queue_size);

        let task = async_std::task::spawn(async move {
            log::info!(
                "[TcpConnectionSender {} => {}] start sending loop",
                peer_id,
                remote_peer_id
            );
            let mut tick_interval = async_std::stream::interval(Duration::from_millis(5000));
            send_tcp_stream(&mut socket, TcpMsg::<MSG>::Ping(timer.now_ms())).await;

            loop {
                let msg: Result<OutgoingEvent<MSG>, RecvError> = select! {
                    e = r_rx.recv().fuse() => e,
                    e = unr_rx.recv().fuse() => e,
                    e = tick_interval.next().fuse() => {
                        log::debug!("[TcpConnectionSender {} => {}] sending Ping", peer_id, remote_peer_id);
                        Ok(OutgoingEvent::Msg(TcpMsg::Ping(timer.now_ms())))
                    }
                };

                match msg {
                    Ok(OutgoingEvent::Msg(msg)) => {
                        if let Err(e) = send_tcp_stream(&mut socket, msg).await {}
                    }
                    Ok(OutgoingEvent::CloseRequest) => {
                        if let Err(e) = socket.shutdown(Shutdown::Both) {
                            log::error!(
                                "[TcpConnectionSender {} => {}] close sender error {}",
                                peer_id,
                                remote_peer_id,
                                e
                            );
                        } else {
                            log::info!(
                                "[TcpConnectionSender {} => {}] close sender loop",
                                peer_id,
                                remote_peer_id
                            );
                        }
                        break;
                    }
                    Ok(OutgoingEvent::ClosedNotify) => {
                        log::info!(
                            "[TcpConnectionSender {} => {}] socket closed",
                            peer_id,
                            remote_peer_id
                        );
                        break;
                    }
                    Err(err) => {
                        log::error!(
                            "[TcpConnectionSender {} => {}] channel error {:?}",
                            peer_id,
                            remote_peer_id,
                            err
                        );
                        break;
                    }
                }
            }
            log::info!(
                "[TcpConnectionSender {} => {}] stop sending loop",
                peer_id,
                remote_peer_id
            );
            ()
        });

        (
            Self {
                remote_addr,
                remote_peer_id,
                conn_id,
                reliable_sender: reliable_sender.clone(),
                unreliable_sender,
                task: Some(task),
            },
            reliable_sender,
        )
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
        if let Err(e) = self
            .unreliable_sender
            .send_blocking(OutgoingEvent::CloseRequest)
        {
            log::error!("[ConnectionSender] send Close request error {:?}", e);
        } else {
            log::info!("[ConnectionSender] sent close request");
        }
    }
}

impl<MSG> Drop for TcpConnectionSender<MSG> {
    fn drop(&mut self) {
        if let Some(mut task) = self.task.take() {
            task.cancel();
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

pub struct TcpConnectionReceiver<MSG> {
    pub(crate) peer_id: PeerId,
    pub(crate) remote_peer_id: PeerId,
    pub(crate) remote_addr: PeerAddr,
    pub(crate) conn_id: u32,
    pub(crate) socket: TcpStream,
    pub(crate) buf: [u8; BUFFER_LEN],
    pub(crate) timer: Arc<dyn Timer>,
    pub(crate) reliable_sender: Sender<OutgoingEvent<MSG>>,
}

#[async_trait::async_trait]
impl<MSG> ConnectionReceiver<MSG> for TcpConnectionReceiver<MSG>
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
            log::debug!(
                "[ConnectionReceiver {} => {}] waiting event",
                self.peer_id,
                self.remote_peer_id
            );
            match recv_tcp_stream::<MSG>(&mut self.buf, &mut self.socket).await {
                Ok(msg) => {
                    match msg {
                        TcpMsg::Msg(service_id, msg) => {
                            break Ok(ConnectionEvent::Msg { service_id, msg });
                        }
                        TcpMsg::Ping(sent_ts) => {
                            log::debug!(
                                "[ConnectionReceiver {} => {}] on Ping => reply Pong",
                                self.peer_id,
                                self.remote_peer_id
                            );
                            self.reliable_sender
                                .send_blocking(OutgoingEvent::Msg(TcpMsg::<MSG>::Pong(sent_ts)));
                        }
                        TcpMsg::Pong(ping_sent_ts) => {
                            //TODO est speed and over_use state
                            log::debug!(
                                "[ConnectionReceiver {} => {}] on Pong",
                                self.peer_id,
                                self.remote_peer_id
                            );
                            break Ok(ConnectionEvent::Stats(ConnectionStats {
                                rtt_ms: (self.timer.now_ms() - ping_sent_ts) as u16,
                                sending_kbps: 0,
                                send_est_kbps: 0,
                                loss_percent: 0,
                                over_use: false,
                            }));
                        }
                        _ => {
                            log::warn!("[ConnectionReceiver {} => {}] wrong msg type, required TcpMsg::Msg", self.peer_id, self.remote_peer_id);
                        }
                    }
                }
                Err(e) => {
                    log::info!(
                        "[ConnectionReceiver {} => {}] stream closed",
                        self.peer_id,
                        self.remote_peer_id
                    );
                    self.reliable_sender
                        .send_blocking(OutgoingEvent::ClosedNotify);
                    break Err(());
                }
            }
        }
    }
}

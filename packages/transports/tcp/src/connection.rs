use crate::msg::TcpMsg;
use async_bincode::futures::AsyncBincodeStream;
use async_bincode::AsyncDestination;
use async_std::channel::{bounded, unbounded, RecvError, Sender};
use async_std::net::{Shutdown, TcpStream};
use async_std::task::JoinHandle;
use bluesea_identity::{ConnId, NodeAddr, NodeId};
use futures_util::{select, FutureExt, SinkExt, StreamExt};
use network::msg::TransportMsg;
use network::transport::{ConnectionEvent, ConnectionReceiver, ConnectionSender, ConnectionStats};
use std::sync::Arc;
use std::time::Duration;
use utils::error_handle::ErrorUtils;
use utils::option_handle::OptionUtils;
use utils::Timer;

pub type AsyncBincodeStreamU16 = AsyncBincodeStream<TcpStream, TcpMsg, TcpMsg, AsyncDestination>;

pub async fn send_tcp_stream(writer: &mut AsyncBincodeStreamU16, msg: TcpMsg) -> Result<(), ()> {
    match writer.send(msg).await {
        Ok(_) => Ok(()),
        Err(err) => {
            log::error!("[TcpTransport] write buffer error {:?}", err);
            Err(())
        }
    }
}

pub enum OutgoingEvent {
    Msg(TcpMsg),
    CloseRequest,
    ClosedNotify,
}

pub struct TcpConnectionSender {
    remote_node_id: NodeId,
    remote_addr: NodeAddr,
    conn_id: ConnId,
    reliable_sender: Sender<OutgoingEvent>,
    unreliable_sender: Sender<OutgoingEvent>,
    task: Option<JoinHandle<()>>,
}

impl TcpConnectionSender {
    pub fn new(
        node_id: NodeId,
        remote_node_id: NodeId,
        remote_addr: NodeAddr,
        conn_id: ConnId,
        unreliable_queue_size: usize,
        mut socket: AsyncBincodeStreamU16,
        timer: Arc<dyn Timer>,
    ) -> (Self, Sender<OutgoingEvent>) {
        let (reliable_sender, r_rx) = unbounded();
        let (unreliable_sender, unr_rx) = bounded(unreliable_queue_size);

        let task = async_std::task::spawn(async move {
            log::info!("[TcpConnectionSender {} => {}] start sending loop", node_id, remote_node_id);
            let mut tick_interval = async_std::stream::interval(Duration::from_millis(5000));
            send_tcp_stream(&mut socket, TcpMsg::Ping(timer.now_ms())).await.print_error("Should send ping");

            loop {
                let msg: Result<OutgoingEvent, RecvError> = select! {
                    e = r_rx.recv().fuse() => e,
                    e = unr_rx.recv().fuse() => e,
                    _ = tick_interval.next().fuse() => {
                        log::debug!("[TcpConnectionSender {} => {}] sending Ping", node_id, remote_node_id);
                        Ok(OutgoingEvent::Msg(TcpMsg::Ping(timer.now_ms())))
                    }
                };

                match msg {
                    Ok(OutgoingEvent::Msg(msg)) => send_tcp_stream(&mut socket, msg).await.print_error("Should send tcp stream"),
                    Ok(OutgoingEvent::CloseRequest) => {
                        if let Err(e) = socket.get_mut().shutdown(Shutdown::Both) {
                            log::error!("[TcpConnectionSender {} => {}] close sender error {}", node_id, remote_node_id, e);
                        } else {
                            log::info!("[TcpConnectionSender {} => {}] close sender loop", node_id, remote_node_id);
                        }
                        break;
                    }
                    Ok(OutgoingEvent::ClosedNotify) => {
                        log::info!("[TcpConnectionSender {} => {}] socket closed", node_id, remote_node_id);
                        break;
                    }
                    Err(err) => {
                        log::error!("[TcpConnectionSender {} => {}] channel error {:?}", node_id, remote_node_id, err);
                        break;
                    }
                }
            }
            log::info!("[TcpConnectionSender {} => {}] stop sending loop", node_id, remote_node_id);
        });

        (
            Self {
                remote_addr,
                remote_node_id,
                conn_id,
                reliable_sender: reliable_sender.clone(),
                unreliable_sender,
                task: Some(task),
            },
            reliable_sender,
        )
    }
}

impl ConnectionSender for TcpConnectionSender {
    fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    fn remote_addr(&self) -> NodeAddr {
        self.remote_addr.clone()
    }

    fn send(&self, msg: TransportMsg) {
        if msg.header.reliable {
            if let Err(e) = self.reliable_sender.send_blocking(OutgoingEvent::Msg(TcpMsg::Msg(msg.take()))) {
                log::error!("[ConnectionSender] send reliable msg error {:?}", e);
            } else {
                log::debug!("[ConnectionSender] send reliable msg");
            }
        } else if let Err(e) = self.unreliable_sender.try_send(OutgoingEvent::Msg(TcpMsg::Msg(msg.take()))) {
            log::error!("[ConnectionSender] send unreliable msg error {:?}", e);
        } else {
            log::debug!("[ConnectionSender] send unreliable msg");
        }
    }

    fn close(&self) {
        if let Err(e) = self.unreliable_sender.send_blocking(OutgoingEvent::CloseRequest) {
            log::error!("[ConnectionSender] send Close request error {:?}", e);
        } else {
            log::info!("[ConnectionSender] sent close request");
        }
    }
}

impl Drop for TcpConnectionSender {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            async_std::task::spawn(async move {
                task.cancel().await.print_none("Should cancel task");
            });
        }
    }
}

pub async fn recv_tcp_stream(reader: &mut AsyncBincodeStreamU16) -> Result<TcpMsg, ()> {
    if let Some(res) = reader.next().await {
        res.map_err(|_| ())
    } else {
        Err(())
    }
}

pub struct TcpConnectionReceiver {
    pub(crate) node_id: NodeId,
    pub(crate) remote_node_id: NodeId,
    pub(crate) remote_addr: NodeAddr,
    pub(crate) conn_id: ConnId,
    pub(crate) socket: AsyncBincodeStreamU16,
    pub(crate) timer: Arc<dyn Timer>,
    pub(crate) reliable_sender: Sender<OutgoingEvent>,
}

#[async_trait::async_trait]
impl ConnectionReceiver for TcpConnectionReceiver {
    fn remote_node_id(&self) -> NodeId {
        self.remote_node_id
    }

    fn conn_id(&self) -> ConnId {
        self.conn_id
    }

    fn remote_addr(&self) -> NodeAddr {
        self.remote_addr.clone()
    }

    async fn poll(&mut self) -> Result<ConnectionEvent, ()> {
        loop {
            log::debug!("[ConnectionReceiver {} => {}] waiting event", self.node_id, self.remote_node_id);
            match recv_tcp_stream(&mut self.socket).await {
                Ok(msg) => {
                    match msg {
                        TcpMsg::Msg(buf) => match TransportMsg::from_vec(buf) {
                            Ok(msg) => break Ok(ConnectionEvent::Msg(msg)),
                            Err(e) => {
                                log::error!("[ConnectionReceiver {} => {}] wrong msg format {:?}", self.node_id, self.remote_node_id, e);
                            }
                        },
                        TcpMsg::Ping(sent_ts) => {
                            log::debug!("[ConnectionReceiver {} => {}] on Ping => reply Pong", self.node_id, self.remote_node_id);
                            self.reliable_sender.send_blocking(OutgoingEvent::Msg(TcpMsg::Pong(sent_ts))).print_error("Should send Pong");
                        }
                        TcpMsg::Pong(ping_sent_ts) => {
                            //TODO est speed and over_use state
                            log::debug!("[ConnectionReceiver {} => {}] on Pong", self.node_id, self.remote_node_id);
                            break Ok(ConnectionEvent::Stats(ConnectionStats {
                                rtt_ms: (self.timer.now_ms() - ping_sent_ts) as u16,
                                sending_kbps: 0,
                                send_est_kbps: 0,
                                loss_percent: 0,
                                over_use: false,
                            }));
                        }
                        _ => {
                            log::warn!("[ConnectionReceiver {} => {}] wrong msg type, required TcpMsg::Msg", self.node_id, self.remote_node_id);
                        }
                    }
                }
                Err(_) => {
                    log::info!("[ConnectionReceiver {} => {}] stream closed", self.node_id, self.remote_node_id);
                    self.reliable_sender.send_blocking(OutgoingEvent::ClosedNotify).print_error("Should send CloseNotify");
                    break Err(());
                }
            }
        }
    }
}

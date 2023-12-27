use crate::msg::TcpMsg;
use async_bincode::futures::AsyncBincodeStream;
use async_bincode::AsyncDestination;
use async_std::channel::{bounded, RecvError, Sender};
use async_std::net::{Shutdown, TcpStream};
use async_std::task::JoinHandle;
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_network::msg::TransportMsg;
use atm0s_sdn_network::transport::{ConnectionEvent, ConnectionReceiver, ConnectionSender, ConnectionStats};
use atm0s_sdn_utils::error_handle::ErrorUtils;
use atm0s_sdn_utils::option_handle::OptionUtils;
use atm0s_sdn_utils::Timer;
use futures_util::{select, FutureExt, SinkExt, StreamExt};
use parking_lot::Mutex;
use snow::TransportState;
use std::sync::Arc;
use std::time::Duration;

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
    unreliable_sender: Sender<OutgoingEvent>,
    task: Option<JoinHandle<()>>,
    snow_state: Arc<Mutex<TransportState>>,
    tmp_buf: Arc<Mutex<[u8; 1500]>>,
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
        snow_state: Arc<Mutex<TransportState>>,
    ) -> (Self, Sender<OutgoingEvent>) {
        let (unreliable_sender, unr_rx) = bounded(unreliable_queue_size);

        let task = async_std::task::spawn(async move {
            log::info!("[TcpConnectionSender {} => {}] start sending loop", node_id, remote_node_id);
            let mut tick_interval = async_std::stream::interval(Duration::from_millis(5000));
            send_tcp_stream(&mut socket, TcpMsg::Ping(timer.now_ms())).await.print_error("Should send ping");

            loop {
                let msg: Result<OutgoingEvent, RecvError> = select! {
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
                unreliable_sender: unreliable_sender.clone(),
                task: Some(task),
                snow_state,
                tmp_buf: Arc::new(Mutex::new([0; 1500])),
            },
            unreliable_sender,
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
        let buf = if msg.header.secure {
            let mut tmp_buf = self.tmp_buf.lock();
            tmp_buf[0] = msg.get_buf()[0];
            let snow_len = self.snow_state.lock().write_message(msg.get_buf(), &mut tmp_buf[1..]).expect("Snow write error");
            tmp_buf[..(1 + snow_len)].to_vec()
        } else {
            msg.take()
        };

        if let Err(e) = self.unreliable_sender.try_send(OutgoingEvent::Msg(TcpMsg::Msg(buf))) {
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
    pub(crate) remote_node_id: NodeId,
    pub(crate) remote_addr: NodeAddr,
    pub(crate) conn_id: ConnId,
    pub(crate) socket: AsyncBincodeStreamU16,
    pub(crate) timer: Arc<dyn Timer>,
    pub(crate) unreliable_sender: Sender<OutgoingEvent>,
    pub(crate) snow_state: Arc<Mutex<TransportState>>,
    pub(crate) snow_buf: [u8; 1500],
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
            log::debug!("[ConnectionReceiver {}/{}] waiting event", self.remote_node_id, self.conn_id);
            match recv_tcp_stream(&mut self.socket).await {
                Ok(msg) => {
                    match msg {
                        TcpMsg::Msg(data) => {
                            if TransportMsg::is_secure_header(data[0]) {
                                let mut snow_state = self.snow_state.lock();
                                if let Ok(len) = snow_state.read_message(&data[1..], &mut self.snow_buf) {
                                    //TODO reduce to_vec memory copy
                                    match TransportMsg::from_vec(self.snow_buf[0..len].to_vec()) {
                                        Ok(msg) => break Ok(ConnectionEvent::Msg(msg)),
                                        Err(e) => {
                                            log::error!("[UdpServerConnectionReceiver {}/{}] wrong msg format {:?}", self.remote_node_id, self.conn_id, e);
                                        }
                                    }
                                }
                            } else {
                                //TODO reduce to_vec memory copy
                                match TransportMsg::from_vec(data) {
                                    Ok(msg) => break Ok(ConnectionEvent::Msg(msg)),
                                    Err(e) => {
                                        log::error!("[UdpServerConnectionReceiver {}/{}] wrong msg format {:?}", self.remote_node_id, self.conn_id, e);
                                    }
                                }
                            }
                        }
                        TcpMsg::Ping(sent_ts) => {
                            log::debug!("[ConnectionReceiver {}/{}] on Ping => reply Pong", self.remote_node_id, self.conn_id);
                            self.unreliable_sender.try_send(OutgoingEvent::Msg(TcpMsg::Pong(sent_ts))).print_error("Should send Pong");
                        }
                        TcpMsg::Pong(ping_sent_ts) => {
                            //TODO est speed and over_use state
                            log::debug!("[ConnectionReceiver {}/{}] on Pong", self.remote_node_id, self.conn_id);
                            break Ok(ConnectionEvent::Stats(ConnectionStats {
                                rtt_ms: (self.timer.now_ms() - ping_sent_ts) as u16,
                                sending_kbps: 0,
                                send_est_kbps: 0,
                                loss_percent: 0,
                                over_use: false,
                            }));
                        }
                        _ => {
                            log::warn!("[ConnectionReceiver {}/{}] wrong msg type, required TcpMsg::Msg", self.remote_node_id, self.conn_id);
                        }
                    }
                }
                Err(_) => {
                    log::info!("[ConnectionReceiver {}/{}] stream closed", self.remote_node_id, self.conn_id);
                    self.unreliable_sender.try_send(OutgoingEvent::ClosedNotify).print_error("Should send CloseNotify");
                    break Err(());
                }
            }
        }
    }
}

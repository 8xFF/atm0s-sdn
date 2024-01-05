use std::{collections::HashMap, sync::Arc};

use async_std::{channel::Receiver, stream::StreamExt, task::JoinHandle};
use futures::{select, FutureExt as _};
use kcp::{Error, Kcp};
use parking_lot::RwLock;

use super::socket::{VirtualSocket, VirtualSocketWriter};

const MAX_KCP_SEND_QUEUE: usize = 10;

enum ReadEvent {
    Continue,
    Close,
}

pub struct VirtualStream {
    meta: HashMap<String, String>,
    kcp: Arc<RwLock<Kcp<VirtualSocketWriter>>>,
    task: Option<JoinHandle<()>>,
    write_awake_rx: Receiver<()>,
    read_awake_rx: Receiver<ReadEvent>,
}

impl VirtualStream {
    pub fn new(socket: VirtualSocket) -> Self {
        let (mut reader, writer) = socket.split();
        let meta = reader.meta().clone();
        let mut kcp = Kcp::new_stream(writer.remote().client_id(), writer);
        kcp.set_nodelay(true, 20, 2, true);

        let kcp = Arc::new(RwLock::new(kcp));
        let (write_awake_tx, write_awake_rx) = async_std::channel::bounded(1);
        let (read_awake_tx, read_awake_rx) = async_std::channel::bounded(1);
        let kcp_c = kcp.clone();
        let task = async_std::task::spawn(async move {
            let mut timer = async_std::stream::interval(std::time::Duration::from_millis(10));
            let started_at = std::time::Instant::now();
            loop {
                select! {
                    _ = timer.next().fuse() => {
                        if let Err(e) = kcp_c.write().update(started_at.elapsed().as_millis() as u32) {
                            log::error!("[VirtualStream] kcp update error: {:?}", e);
                            break;
                        }
                    }
                    e = reader.read().fuse() => {
                        if let Some(buf) = e {
                            if buf.len() == 0 {
                                log::info!("[VirtualStream] reader closed");
                                read_awake_tx.try_send(ReadEvent::Close).ok();
                                break;
                            }

                            let mut kcp = kcp_c.write();
                            if let Err(e) = kcp.input(&buf) {
                                log::error!("[VirtualStream] kcp input error: {:?}", e);
                                break;
                            } else {
                                if let Ok(len) = kcp.peeksize() {
                                    if len > 0 {
                                        read_awake_tx.try_send(ReadEvent::Continue).ok();
                                    }
                                }
                                if kcp.wait_snd() < MAX_KCP_SEND_QUEUE {
                                    write_awake_tx.try_send(()).ok();
                                }
                            }
                        } else {
                            log::info!("[VirtualStream] reader closed");
                            read_awake_tx.try_send(ReadEvent::Close).ok();
                            break;
                        }
                    }
                }
            }
        });

        Self {
            meta,
            kcp,
            task: Some(task),
            write_awake_rx,
            read_awake_rx,
        }
    }

    pub fn meta(&self) -> &HashMap<String, String> {
        &self.meta
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        loop {
            let kcp_wait_snd = self.kcp.read().wait_snd();
            if kcp_wait_snd < MAX_KCP_SEND_QUEUE {
                self.kcp.write().send(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                return Ok(());
            } else {
                self.write_awake_rx.recv().await.map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "ConnectionInterrupted"))?;
            }
        }
    }

    pub async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let size = match self.kcp.write().recv(buf) {
                Ok(size) => size,
                Err(e) => match e {
                    Error::RecvQueueEmpty => 0,
                    _ => {
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
                    }
                },
            };
            if size > 0 {
                return Ok(size);
            } else {
                match self.read_awake_rx.recv().await {
                    Ok(ReadEvent::Continue) => {}
                    Ok(ReadEvent::Close) => return Ok(0),
                    Err(_) => return Err(std::io::Error::new(std::io::ErrorKind::Other, "ConnectionInterrupted")),
                }
            }
        }
    }
}

impl Drop for VirtualStream {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            async_std::task::spawn(async {
                task.cancel().await;
            });
        }
    }
}

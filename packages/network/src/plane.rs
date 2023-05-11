use crate::behaviour::NetworkBehavior;
use crate::transport::{
    ConnectionSender, Transport, TransportAddr, TransportEvent, TransportPendingOutgoing,
};
use async_std::channel::{bounded, Receiver, Sender};
use futures::{select, FutureExt, SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;

pub struct NetworkAgent {}

impl NetworkAgent {
    pub fn connect_to(&self, addr: TransportAddr) -> TransportPendingOutgoing {
        todo!()
    }
}

enum NetworkPlaneInternalEvent {
    IncomingDisconnected(Arc<dyn ConnectionSender>),
    OutgoingDisconnected(Arc<dyn ConnectionSender>),
}

pub struct NetworkPlaneConfig {
    behavior: Vec<Box<dyn NetworkBehavior>>,
    transport: Box<dyn Transport>,
}

pub struct NetworkPlane {
    conf: NetworkPlaneConfig,
    internal_tx: Sender<NetworkPlaneInternalEvent>,
    internal_rx: Receiver<NetworkPlaneInternalEvent>,
}

impl NetworkPlane {
    pub fn new(conf: NetworkPlaneConfig) -> Self {
        let (internal_tx, internal_rx) = bounded(1);
        Self {
            conf,
            internal_tx,
            internal_rx,
        }
    }

    pub async fn run(&mut self) -> Result<(), ()> {
        select! {
            e = self.conf.transport.recv().fuse() => {
                let (outgoing, sender, mut receiver) = match e? {
                    TransportEvent::Incoming(sender, receiver) => {
                        let mut handlers = vec![];
                        for behaviour in &mut self.conf.behavior {
                            if let Some(handler) = behaviour.on_incoming_connection_connected(sender.clone()) {
                                handlers.push(handler);
                            }
                        }
                        (false, sender, receiver)
                    }
                    TransportEvent::Outgoing(sender, receiver) => {
                        let mut handlers = vec![];
                        for behaviour in &mut self.conf.behavior {
                            if let Some(handler) = behaviour.on_incoming_connection_connected(sender.clone()) {
                                handlers.push(handler);
                            }
                        }
                        (true, sender, receiver)
                    }
                    TransportEvent::OutgoingError { connection_id, err } => {
                        for behaviour in &mut self.conf.behavior {
                            behaviour.on_outgoing_connection_error(connection_id, &err);
                        }
                        return Ok(());
                    }
                };

                let internal_tx = self.internal_tx.clone();
                async_std::task::spawn(async move {
                    loop {
                        match receiver.poll().await {
                            Ok(event) => {

                            }
                            Err(err) => {
                                if outgoing {
                                    if let Err(err) = internal_tx.send(NetworkPlaneInternalEvent::IncomingDisconnected(sender)).await {
                                        log::error!("Sending IncomingDisconnected error {:?}", err);
                                    }
                                } else {
                                    if let Err(err) = internal_tx.send(NetworkPlaneInternalEvent::OutgoingDisconnected(sender)).await {
                                        log::error!("Sending OutgoingDisconnected error {:?}", err);
                                    }
                                }
                                break;
                            }
                        }
                    }
                });

                Ok(())
            }
            e =  self.internal_rx.next().fuse() => match e {
                Some(NetworkPlaneInternalEvent::IncomingDisconnected(sender)) => {
                    for behaviour in &mut self.conf.behavior {
                        behaviour.on_incoming_connection_disconnected(sender.clone());
                    }
                    Ok(())
                },
                Some(NetworkPlaneInternalEvent::OutgoingDisconnected(sender)) => {
                    for behaviour in &mut self.conf.behavior {
                        behaviour.on_outgoing_connection_disconnected(sender.clone());
                    }
                    Ok(())
                },
                None => {
                    Err(())
                }
            }
        }
    }
}

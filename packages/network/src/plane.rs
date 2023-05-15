use crate::behaviour::NetworkBehavior;
use crate::transport::{ConnectionSender, OutgoingConnectionError, Transport, TransportConnector, TransportEvent, TransportPendingOutgoing};
use async_std::channel::{bounded, Receiver, Sender};
use futures::{select, FutureExt, SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use async_std::stream::Interval;
use bluesea_identity::{PeerAddr, PeerId};
use utils::Timer;

pub struct NetworkAgent {
    connector: Box<dyn TransportConnector>
}

impl NetworkAgent {
    pub fn new(connector: Box<dyn TransportConnector>) -> Self {
        Self {
            connector
        }
    }

    pub fn connect_to(&self, peer_id: PeerId, dest: PeerAddr) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        self.connector.connect_to(peer_id, dest)
    }
}

enum NetworkPlaneInternalEvent {
    IncomingDisconnected(Arc<dyn ConnectionSender>),
    OutgoingDisconnected(Arc<dyn ConnectionSender>),
}

pub struct NetworkPlaneConfig {
    tick_ms: u64,
    behavior: Vec<Box<dyn NetworkBehavior>>,
    transport: Box<dyn Transport>,
    timer: Arc<dyn Timer>,
}

pub struct NetworkPlane {
    agent: Arc<NetworkAgent>,
    conf: NetworkPlaneConfig,
    internal_tx: Sender<NetworkPlaneInternalEvent>,
    internal_rx: Receiver<NetworkPlaneInternalEvent>,
    tick_interval: Interval,
}

impl NetworkPlane {
    pub fn new(conf: NetworkPlaneConfig) -> Self {
        let (internal_tx, internal_rx) = bounded(1);
        Self {
            agent: NetworkAgent::new(conf.transport.connector()).into(),
            tick_interval: async_std::stream::interval(Duration::from_millis(conf.tick_ms)),
            conf,
            internal_tx,
            internal_rx,
        }
    }

    pub async fn run(&mut self) -> Result<(), ()> {
        select! {
            e = self.tick_interval.next().fuse() => {
                let ts_ms = self.conf.timer.now_ms();
                for behaviour in &mut self.conf.behavior {
                    behaviour.on_tick(&self.agent, ts_ms, self.conf.tick_ms);
                }
                Ok(())
            }
            e = self.conf.transport.recv().fuse() => {
                let (outgoing, sender, mut receiver, mut handlers) = match e? {
                    TransportEvent::Incoming(sender, receiver) => {
                        let mut handlers = vec![];
                        for behaviour in &mut self.conf.behavior {
                            if let Some(handler) = behaviour.on_incoming_connection_connected(&self.agent, sender.clone()) {
                                handlers.push(handler);
                            }
                        }
                        (false, sender, receiver, handlers)
                    }
                    TransportEvent::Outgoing(sender, receiver) => {
                        let mut handlers = vec![];
                        for behaviour in &mut self.conf.behavior {
                            if let Some(handler) = behaviour.on_outgoing_connection_connected(&self.agent, sender.clone()) {
                                handlers.push(handler);
                            }
                        }
                        (true, sender, receiver, handlers)
                    }
                    TransportEvent::OutgoingError { peer_id, connection_id, err } => {
                        for behaviour in &mut self.conf.behavior {
                            behaviour.on_outgoing_connection_error(&self.agent, peer_id, connection_id, &err);
                        }
                        return Ok(());
                    }
                };

                let internal_tx = self.internal_tx.clone();
                let tick_ms = self.conf.tick_ms;
                let timer = self.conf.timer.clone();
                let agent = self.agent.clone();
                async_std::task::spawn(async move {
                    for handler in &mut handlers {
                        handler.on_opened(&agent);
                    }
                    let mut tick_interval = async_std::stream::interval(Duration::from_millis(tick_ms));
                    loop {
                        select! {
                            e = tick_interval.next().fuse() => {
                                let ts_ms = timer.now_ms();
                                for handler in &mut handlers {
                                    handler.on_tick(&agent, ts_ms, tick_ms);
                                }
                            }
                            e = receiver.poll().fuse() => match e {
                                Ok(event) => {
                                    for handler in &mut handlers {
                                        handler.on_event(&agent, &event);
                                    }
                                }
                                Err(err) => {
                                    break;
                                }
                            }
                        }
                    }
                    for handler in &mut handlers {
                        handler.on_closed(&agent);
                    }
                    if outgoing {
                        if let Err(err) = internal_tx.send(NetworkPlaneInternalEvent::IncomingDisconnected(sender)).await {
                            log::error!("Sending IncomingDisconnected error {:?}", err);
                        }
                    } else {
                        if let Err(err) = internal_tx.send(NetworkPlaneInternalEvent::OutgoingDisconnected(sender)).await {
                            log::error!("Sending OutgoingDisconnected error {:?}", err);
                        }
                    }
                });

                Ok(())
            }
            e =  self.internal_rx.next().fuse() => match e {
                Some(NetworkPlaneInternalEvent::IncomingDisconnected(sender)) => {
                    for behaviour in &mut self.conf.behavior {
                        behaviour.on_incoming_connection_disconnected(&self.agent, sender.clone());
                    }
                    Ok(())
                },
                Some(NetworkPlaneInternalEvent::OutgoingDisconnected(sender)) => {
                    for behaviour in &mut self.conf.behavior {
                        behaviour.on_outgoing_connection_disconnected(&self.agent, sender.clone());
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use bluesea_identity::PeerId;
    use crate::behaviour::{ConnectionHandler, NetworkBehavior, NetworkBehaviorEvent};
    use crate::plane::NetworkAgent;
    use crate::transport::{ConnectionSender, OutgoingConnectionError};

    enum MockNetworkEvent {
        IncomingConnected(u32),
        OutgoingConnected(u32),
        IncomingDisconnected(u32),
        OutgoingDisconnected(u32),
    }

    struct MockNetworkBehavior {

    }


    impl NetworkBehavior for MockNetworkBehavior {
        fn on_tick(&mut self, agent: &NetworkAgent, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler>> {
            connection.close();
            None
        }

        fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler>> {
            connection.close();
            None
        }

        fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {
            todo!()
        }

        fn on_event(&mut self, agent: &NetworkAgent, event: NetworkBehaviorEvent) {
            todo!()
        }
    }

    // // Test is it correct to delivery incoming connection event
    // #[async_std::test]
    // async fn incoming_connection() {
    //
    // }
    //
    // // Test is it correct to delivery outgoing connection event
    // #[async_std::test]
    // async fn outgoing_connection() {
    //
    // }
}
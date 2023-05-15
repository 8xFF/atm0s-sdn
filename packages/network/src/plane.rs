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

pub struct NetworkAgent<HE> {
    connector: Box<dyn TransportConnector>,
    tmp: Option<HE>
}

impl<HE> NetworkAgent<HE> {
    pub fn new(connector: Box<dyn TransportConnector>) -> Self {
        Self {
            connector,
            tmp: None,
        }
    }

    pub fn connect_to(&self, peer_id: PeerId, dest: PeerAddr) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        self.connector.connect_to(peer_id, dest)
    }

    pub fn send_handler(&self, peer_id: PeerId, connection_id: Option<u32>, event: HE) {
        todo!()
    }
}

pub struct ConnectionAgent<BE> {
    tmp: Option<BE>,
}

impl<BE> ConnectionAgent<BE> {
    pub fn new() -> Self {
        Self {
            tmp: None
        }
    }

    pub fn send_behavior(&self, event: BE) {
        todo!()
    }
}

enum NetworkPlaneInternalEvent {
    IncomingDisconnected(Arc<dyn ConnectionSender>),
    OutgoingDisconnected(Arc<dyn ConnectionSender>),
}

pub struct NetworkPlaneConfig<BE, HE> {
    tick_ms: u64,
    behavior: Vec<Box<dyn NetworkBehavior<BE, HE>>>,
    transport: Box<dyn Transport>,
    timer: Arc<dyn Timer>,
}

pub struct NetworkPlane<BE, HE> {
    agent: Arc<NetworkAgent<HE>>,
    conf: NetworkPlaneConfig<BE, HE>,
    internal_tx: Sender<NetworkPlaneInternalEvent>,
    internal_rx: Receiver<NetworkPlaneInternalEvent>,
    tick_interval: Interval,
}

impl<BE, HE> NetworkPlane<BE, HE>
    where BE: Send + Sync + 'static
{
    pub fn new(conf: NetworkPlaneConfig<BE, HE>) -> Self {
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
                    let conn_agent = ConnectionAgent::<BE>::new();
                    for handler in &mut handlers {
                        handler.on_opened(&conn_agent);
                    }
                    let mut tick_interval = async_std::stream::interval(Duration::from_millis(tick_ms));
                    loop {
                        select! {
                            e = tick_interval.next().fuse() => {
                                let ts_ms = timer.now_ms();
                                for handler in &mut handlers {
                                    handler.on_tick(&conn_agent, ts_ms, tick_ms);
                                }
                            }
                            e = receiver.poll().fuse() => match e {
                                Ok(event) => {
                                    for handler in &mut handlers {
                                        handler.on_event(&conn_agent, &event);
                                    }
                                }
                                Err(err) => {
                                    break;
                                }
                            }
                        }
                    }
                    for handler in &mut handlers {
                        handler.on_closed(&conn_agent);
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
    use utils::SystemTimer;
    use crate::behaviour::{ConnectionHandler, NetworkBehavior, NetworkBehaviorEvent};
    use crate::mock::MockTransport;
    use crate::plane::{ConnectionAgent, NetworkAgent, NetworkPlane, NetworkPlaneConfig};
    use crate::transport::{ConnectionEvent, ConnectionSender, OutgoingConnectionError};

    enum Behavior1Event {}
    enum Handler1Event {}

    enum Behavior2Event {}
    enum Handler2Event {}

    enum ImplNetworkBehaviorEvent {
        Service1(Behavior1Event),
        Service2(Behavior2Event)
    }

    impl From<Behavior1Event> for ImplNetworkBehaviorEvent {
        fn from(value: Behavior1Event) -> Self {
            ImplNetworkBehaviorEvent::Service1(value)
        }
    }

    impl TryInto<Behavior1Event> for ImplNetworkBehaviorEvent {
        type Error = ();

        fn try_into(self) -> Result<Behavior1Event, Self::Error> {
            match self {
                ImplNetworkBehaviorEvent::Service1(e) => {
                    Ok(e)
                },
                _ => Err(())
            }
        }
    }

    impl From<Behavior2Event> for ImplNetworkBehaviorEvent {
        fn from(value: Behavior2Event) -> Self {
            ImplNetworkBehaviorEvent::Service2(value)
        }
    }

    impl TryInto<Behavior2Event> for ImplNetworkBehaviorEvent {
        type Error = ();

        fn try_into(self) -> Result<Behavior2Event, Self::Error> {
            match self {
                ImplNetworkBehaviorEvent::Service2(e) => {
                    Ok(e)
                },
                _ => Err(())
            }
        }
    }

    enum ImplNetworkHandlerEvent {
        Service1(Handler1Event),
        Service2(Handler2Event),
    }

    impl From<Handler1Event> for ImplNetworkHandlerEvent {
        fn from(value: Handler1Event) -> Self {
            ImplNetworkHandlerEvent::Service1(value)
        }
    }

    impl TryInto<Handler1Event> for ImplNetworkHandlerEvent {
        type Error = ();

        fn try_into(self) -> Result<Handler1Event, Self::Error> {
            match self {
                ImplNetworkHandlerEvent::Service1(e) => {
                    Ok(e)
                },
                _ => Err(())
            }
        }
    }

    impl From<Handler2Event> for ImplNetworkHandlerEvent {
        fn from(value: Handler2Event) -> Self {
            ImplNetworkHandlerEvent::Service2(value)
        }
    }

    impl TryInto<Handler2Event> for ImplNetworkHandlerEvent {
        type Error = ();

        fn try_into(self) -> Result<Handler2Event, Self::Error> {
            match self {
                ImplNetworkHandlerEvent::Service2(e) => {
                    Ok(e)
                },
                _ => Err(())
            }
        }
    }

    struct Test1NetworkBehavior {}

    struct Test1NetworkHandler {}

    struct Test2NetworkBehavior {}

    struct Test2NetworkHandler {}

    impl<BE, HE> NetworkBehavior<BE, HE> for Test1NetworkBehavior
        where BE: From<Behavior1Event> + TryInto<Behavior1Event>,
              HE: From<Handler1Event> + TryInto<Handler1Event>,
    {
        fn on_tick(&mut self, agent: &NetworkAgent<HE>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE>>> {
            Some(Box::new(Test1NetworkHandler {}))
        }

        fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE>>> {
            Some(Box::new(Test1NetworkHandler {}))
        }

        fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {
            todo!()
        }

        fn on_event(&mut self, agent: &NetworkAgent<HE>, event: NetworkBehaviorEvent) {
            todo!()
        }

        fn on_handler_event(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, event: HE) {
            if let Ok(event) = event.try_into() {

            }
        }
    }

    impl<BE> ConnectionHandler<BE> for Test1NetworkHandler
        where BE: From<Behavior1Event> + TryInto<Behavior1Event>
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE>) {
            todo!()
        }

        fn on_tick(&mut self, agent: &ConnectionAgent<BE>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_event(&mut self, agent: &ConnectionAgent<BE>, event: &ConnectionEvent) {
            todo!()
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE>, event: BE) {
            if let Ok(event) = event.try_into() {

            }
        }

        fn on_closed(&mut self, agent: &ConnectionAgent<BE>) {
            todo!()
        }
    }

    impl<BE, HE> NetworkBehavior<BE, HE> for Test2NetworkBehavior
        where BE: From<Behavior2Event> + TryInto<Behavior2Event>,
              HE: From<Handler2Event> + TryInto<Handler2Event>,
    {
        fn on_tick(&mut self, agent: &NetworkAgent<HE>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }

        fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }

        fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {
            todo!()
        }

        fn on_event(&mut self, agent: &NetworkAgent<HE>, event: NetworkBehaviorEvent) {
            todo!()
        }

        fn on_handler_event(&mut self, agent: &NetworkAgent<HE>, peer_id: PeerId, connection_id: u32, event: HE) {
            if let Ok(event) = event.try_into() {

            }
        }
    }

    impl<BE> ConnectionHandler<BE> for Test2NetworkHandler
        where BE: From<Behavior2Event> + TryInto<Behavior2Event>
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE>) {
            todo!()
        }

        fn on_tick(&mut self, agent: &ConnectionAgent<BE>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_event(&mut self, agent: &ConnectionAgent<BE>, event: &ConnectionEvent) {
            todo!()
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE>, event: BE) {
            if let Ok(event) = event.try_into() {

            }
        }

        fn on_closed(&mut self, agent: &ConnectionAgent<BE>) {
            todo!()
        }
    }

    #[async_std::test]
    async fn create_network() {
        let behavior1 = Box::new(Test1NetworkBehavior {});
        let behavior2 = Box::new(Test2NetworkBehavior {});

        let transport = Box::new(MockTransport {});
        let timer = Arc::new(SystemTimer());
        
        let mut plane = NetworkPlane::<ImplNetworkBehaviorEvent, ImplNetworkHandlerEvent>::new(NetworkPlaneConfig {
            tick_ms: 1000,
            behavior: vec![behavior1, behavior2],
            transport,
            timer,
        });

        plane.run().await;
    }
}
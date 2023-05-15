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

pub struct NetworkAgent<HE, MSG> {
    connector: Box<dyn TransportConnector>,
    tmp: Option<HE>,
    tmp2: Option<MSG>,
}

impl<HE, MSG> NetworkAgent<HE, MSG> {
    pub fn new(connector: Box<dyn TransportConnector>) -> Self {
        Self {
            connector,
            tmp: None,
            tmp2: None,
        }
    }

    pub fn connect_to(&self, peer_id: PeerId, dest: PeerAddr) -> Result<TransportPendingOutgoing, OutgoingConnectionError> {
        self.connector.connect_to(peer_id, dest)
    }

    pub fn send_handler(&self, peer_id: PeerId, connection_id: Option<u32>, event: HE) {
        todo!()
    }

    pub fn send_to(&self, peer_id: PeerId, connection_id: Option<u32>, stream_id: u16, msg: MSG) {
        todo!()
    }
}

pub struct ConnectionAgent<BE, MSG> {
    tmp: Option<BE>,
    tmp2: Option<MSG>,
}

impl<BE, MSG> ConnectionAgent<BE, MSG> {
    pub fn new() -> Self {
        Self {
            tmp: None,
            tmp2: None
        }
    }

    pub fn send_behavior(&self, event: BE) {
        todo!()
    }

    pub fn send_to(&self, peer_id: PeerId, connection_id: Option<u32>, msg: MSG) {
        todo!()
    }
}

enum NetworkPlaneInternalEvent {
    IncomingDisconnected(Arc<dyn ConnectionSender>),
    OutgoingDisconnected(Arc<dyn ConnectionSender>),
}

pub struct NetworkPlaneConfig<BE, HE, MSG> {
    tick_ms: u64,
    behavior: Vec<Box<dyn NetworkBehavior<BE, HE, MSG>>>,
    transport: Box<dyn Transport<MSG>>,
    timer: Arc<dyn Timer>,
}

pub struct NetworkPlane<BE, HE, MSG> {
    agent: Arc<NetworkAgent<HE, MSG>>,
    conf: NetworkPlaneConfig<BE, HE, MSG>,
    internal_tx: Sender<NetworkPlaneInternalEvent>,
    internal_rx: Receiver<NetworkPlaneInternalEvent>,
    tick_interval: Interval,
}

impl<BE, HE, MSG> NetworkPlane<BE, HE, MSG>
    where BE: Send + Sync + 'static,
          MSG: Send + Sync + 'static
{
    pub fn new(conf: NetworkPlaneConfig<BE, HE, MSG>) -> Self {
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
                    let conn_agent = ConnectionAgent::<BE, MSG>::new();
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

    enum Behavior1Msg {}
    enum Behavior1Event {}
    enum Handler1Event {}

    enum Behavior2Msg {}
    enum Behavior2Event {}
    enum Handler2Event {}

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplNetworkBehaviorEvent {
        Service1(Behavior1Event),
        Service2(Behavior2Event)
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplNetworkHandlerEvent {
        Service1(Handler1Event),
        Service2(Handler2Event),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplNetworkMsg {
        Service1(Behavior1Msg),
        Service2(Behavior2Msg),
    }

    struct Test1NetworkBehavior {}

    struct Test1NetworkHandler {}

    struct Test2NetworkBehavior {}

    struct Test2NetworkHandler {}

    impl<BE, HE, MSG> NetworkBehavior<BE, HE, MSG> for Test1NetworkBehavior
        where BE: From<Behavior1Event> + TryInto<Behavior1Event>,
              HE: From<Handler1Event> + TryInto<Handler1Event>,
              MSG: From<Behavior1Msg> + TryInto<Behavior1Msg>,
    {
        fn on_tick(&mut self, agent: &NetworkAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            Some(Box::new(Test1NetworkHandler {}))
        }

        fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            Some(Box::new(Test1NetworkHandler {}))
        }

        fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {
            todo!()
        }

        fn on_event(&mut self, agent: &NetworkAgent<HE, MSG>, event: NetworkBehaviorEvent) {
            todo!()
        }

        fn on_handler_event(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, event: HE) {
            if let Ok(event) = event.try_into() {

            }
        }
    }

    impl<BE, MSG> ConnectionHandler<BE, MSG> for Test1NetworkHandler
        where BE: From<Behavior1Event> + TryInto<Behavior1Event>,
              MSG: From<Behavior1Msg> + TryInto<Behavior1Msg>,
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, MSG>) {
            todo!()
        }

        fn on_tick(&mut self, agent: &ConnectionAgent<BE, MSG>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: &ConnectionEvent<MSG>) {
            todo!()
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: BE) {
            if let Ok(event) = event.try_into() {

            }
        }

        fn on_closed(&mut self, agent: &ConnectionAgent<BE, MSG>) {
            todo!()
        }
    }

    impl<BE, HE, MSG> NetworkBehavior<BE, HE, MSG> for Test2NetworkBehavior
        where BE: From<Behavior2Event> + TryInto<Behavior2Event>,
              HE: From<Handler2Event> + TryInto<Handler2Event>,
              MSG: From<Behavior2Msg> + TryInto<Behavior2Msg>,
    {
        fn on_tick(&mut self, agent: &NetworkAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }

        fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }

        fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender>) {
            todo!()
        }

        fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {
            todo!()
        }

        fn on_event(&mut self, agent: &NetworkAgent<HE, MSG>, event: NetworkBehaviorEvent) {
            todo!()
        }

        fn on_handler_event(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, event: HE) {
            if let Ok(event) = event.try_into() {

            }
        }
    }

    impl<BE, MSG> ConnectionHandler<BE, MSG> for Test2NetworkHandler
        where BE: From<Behavior2Event> + TryInto<Behavior2Event>,
              MSG: From<Behavior2Msg> + TryInto<Behavior2Msg>,
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, MSG>) {
            todo!()
        }

        fn on_tick(&mut self, agent: &ConnectionAgent<BE, MSG>, ts_ms: u64, interal_ms: u64) {
            todo!()
        }

        fn on_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: &ConnectionEvent<MSG>) {
            todo!()
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: BE) {
            if let Ok(event) = event.try_into() {

            }
        }

        fn on_closed(&mut self, agent: &ConnectionAgent<BE, MSG>) {
            todo!()
        }
    }

    #[async_std::test]
    async fn create_network() {
        let behavior1 = Box::new(Test1NetworkBehavior {});
        let behavior2 = Box::new(Test2NetworkBehavior {});

        let (mock, faker, output) = MockTransport::<ImplNetworkMsg>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());
        
        let mut plane = NetworkPlane::<ImplNetworkBehaviorEvent, ImplNetworkHandlerEvent, ImplNetworkMsg>::new(NetworkPlaneConfig {
            tick_ms: 1000,
            behavior: vec![behavior1, behavior2],
            transport,
            timer,
        });

        plane.run().await;
    }
}
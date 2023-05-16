use crate::behaviour::{NetworkBehavior, ConnectionHandler};
use crate::transport::{ConnectionEvent, ConnectionMsg, ConnectionSender, OutgoingConnectionError, Transport, TransportConnector, TransportEvent, TransportPendingOutgoing};
use async_std::channel::{bounded, Receiver, Sender};
use futures::{select, FutureExt, SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use async_std::stream::Interval;
use bluesea_identity::{PeerAddr, PeerId};
use utils::Timer;

fn init_vec<T>(size: usize, builder: fn() -> T) -> Vec<T> {
    let mut vec = vec![];
    for _ in 0..size {
        vec.push(builder());
    }
    vec
}

pub struct NetworkAgent<HE, MSG> {
    local_peer_id: PeerId,
    connector: Box<dyn TransportConnector>,
    tmp: Option<HE>,
    tmp2: Option<MSG>,
}

impl<HE, MSG> NetworkAgent<HE, MSG> {
    pub fn new(local_peer_id: PeerId, connector: Box<dyn TransportConnector>) -> Self {
        Self {
            connector,
            local_peer_id,
            tmp: None,
            tmp2: None,
        }
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
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
    service_id: u8,
    local_peer_id: PeerId,
    remote_peer_id: PeerId,
    conn_id: u32,
    sender: Arc<dyn ConnectionSender<MSG>>,
    tmp: Option<BE>,
}

impl<BE, MSG> ConnectionAgent<BE, MSG> {
    pub fn new(
        service_id: u8,
        local_peer_id: PeerId,
        remote_peer_id: PeerId,
        conn_id: u32,
        sender: Arc<dyn ConnectionSender<MSG>>
    ) -> Self {
        Self {
            service_id,
            local_peer_id,
            remote_peer_id,
            conn_id,
            tmp: None,
            sender
        }
    }

    pub fn conn_id(&self) -> u32 {
        self.conn_id
    }

    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    pub fn remote_peer_id(&self) -> PeerId {
        self.remote_peer_id
    }

    pub fn send_behavior(&self, event: BE) {
        todo!()
    }

    pub fn send_to(&self, peer_id: PeerId, connection_id: Option<u32>, msg: MSG) {
        todo!()
    }

    pub fn send(&self, msg: ConnectionMsg<MSG>) {
        self.sender.send(self.service_id, msg);
    }
}

enum NetworkPlaneInternalEvent<MSG> {
    IncomingDisconnected(Arc<dyn ConnectionSender<MSG>>),
    OutgoingDisconnected(Arc<dyn ConnectionSender<MSG>>),
}

pub struct NetworkPlaneConfig<BE, HE, MSG> {
    local_peer_id: PeerId,
    tick_ms: u64,
    behavior: Vec<Box<dyn NetworkBehavior<BE, HE, MSG> + Send + Sync>>,
    transport: Box<dyn Transport<MSG> + Send + Sync>,
    timer: Arc<dyn Timer>,
}

pub struct NetworkPlane<BE, HE, MSG> {
    agent: Arc<NetworkAgent<HE, MSG>>,
    conf: NetworkPlaneConfig<BE, HE, MSG>,
    internal_tx: Sender<NetworkPlaneInternalEvent<MSG>>,
    internal_rx: Receiver<NetworkPlaneInternalEvent<MSG>>,
    tick_interval: Interval,
}

impl<BE, HE, MSG> NetworkPlane<BE, HE, MSG>
    where BE: Send + Sync + 'static,
          MSG: Send + Sync + 'static
{
    pub fn new(conf: NetworkPlaneConfig<BE, HE, MSG>) -> Self {
        let (internal_tx, internal_rx) = bounded(1);
        Self {
            agent: NetworkAgent::new(conf.local_peer_id, conf.transport.connector()).into(),
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
                        let mut handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, MSG>>, ConnectionAgent::<BE, MSG>)>> = init_vec(256, || None);
                        for behaviour in &mut self.conf.behavior {
                            let conn_agent = ConnectionAgent::<BE, MSG>::new(
                                behaviour.service_id(),
                                self.conf.local_peer_id,
                                receiver.remote_peer_id(),
                                receiver.connection_id(),
                                sender.clone()
                            );
                            handlers[behaviour.service_id() as usize] = behaviour.on_incoming_connection_connected(&self.agent, sender.clone()).map(|h| (h, conn_agent));
                        }
                        (false, sender, receiver, handlers)
                    }
                    TransportEvent::Outgoing(sender, receiver) => {
                        let mut handlers: Vec<Option<(Box<dyn ConnectionHandler<BE, MSG>>, ConnectionAgent::<BE, MSG>)>> = init_vec(256, || None);
                        for behaviour in &mut self.conf.behavior {
                            let conn_agent = ConnectionAgent::<BE, MSG>::new(
                                behaviour.service_id(),
                                self.conf.local_peer_id,
                                receiver.remote_peer_id(),
                                receiver.connection_id(),
                                sender.clone()
                            );
                            handlers[behaviour.service_id() as usize] = behaviour.on_incoming_connection_connected(&self.agent, sender.clone()).map(|h| (h, conn_agent));
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
                        if let Some((handler, conn_agent)) = handler {
                            handler.on_opened(conn_agent);
                        }
                    }
                    let mut tick_interval = async_std::stream::interval(Duration::from_millis(tick_ms));
                    loop {
                        select! {
                            e = tick_interval.next().fuse() => {
                                let ts_ms = timer.now_ms();
                                for handler in &mut handlers {
                                    if let Some((handler, conn_agent)) = handler {
                                        handler.on_tick(conn_agent, ts_ms, tick_ms);
                                    }
                                }
                            }
                            e = receiver.poll().fuse() => match e {
                                Ok(event) => {
                                    match &event {
                                        ConnectionEvent::Msg { service_id, .. } => {
                                            if let Some((handler, conn_agent)) = &mut handlers[*service_id as usize] {
                                                handler.on_event(&conn_agent, event);
                                            }
                                        }
                                        ConnectionEvent::Stats(stats) => {
                                            for handler in &mut handlers {
                                                if let Some((handler, conn_agent)) = handler {
                                                    handler.on_event(&conn_agent, ConnectionEvent::Stats(stats.clone()));
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    break;
                                }
                            }
                        }
                    }
                    for handler in &mut handlers {
                        if let Some((handler, conn_agent)) = handler {
                            handler.on_closed(&conn_agent);
                        }
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
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use parking_lot::Mutex;
    use bluesea_identity::PeerId;
    use utils::SystemTimer;
    use crate::behaviour::{ConnectionHandler, NetworkBehavior, NetworkBehaviorEvent};
    use crate::mock::{MockInput, MockOutput, MockTransport};
    use crate::plane::{ConnectionAgent, NetworkAgent, NetworkPlane, NetworkPlaneConfig};
    use crate::transport::{ConnectionEvent, ConnectionMsg, ConnectionSender, OutgoingConnectionError};

    #[derive(PartialEq, Debug)]
    enum Behavior1Msg {
        Ping,
        Pong
    }
    enum Behavior1Event {}
    enum Handler1Event {}

    #[derive(PartialEq, Debug)]
    enum Behavior2Msg {
        Ping,
        Pong
    }

    #[derive(PartialEq, Debug)]
    enum DebugInput<MSG> {
        Opened(PeerId, u32),
        Msg(PeerId, u32, MSG),
        Closed(PeerId, u32),
    }

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

    #[derive(PartialEq, Debug, convert_enum::From, convert_enum::TryInto)]
    enum ImplNetworkMsg {
        Service1(Behavior1Msg),
        Service2(Behavior2Msg),
    }

    struct Test1NetworkBehavior<MSG> {
        conn_counter: Arc<AtomicU32>,
        input: Arc<Mutex<VecDeque<DebugInput<MSG>>>>,
    }

    struct Test1NetworkHandler<MSG> {
        input: Arc<Mutex<VecDeque<DebugInput<MSG>>>>,
    }

    struct Test2NetworkBehavior {}

    struct Test2NetworkHandler {}

    impl<BE, HE, MSG> NetworkBehavior<BE, HE, MSG> for Test1NetworkBehavior<MSG>
        where BE: From<Behavior1Event> + TryInto<Behavior1Event>,
              HE: From<Handler1Event> + TryInto<Handler1Event>,
              MSG: From<Behavior1Msg> + TryInto<Behavior1Msg> + Send + Sync + 'static,
    {
        fn service_id(&self) -> u8 {
            0
        }
        fn on_tick(&mut self, agent: &NetworkAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {}
        fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            self.conn_counter.fetch_add(1, Ordering::Relaxed);
            Some(Box::new(Test1NetworkHandler {
                input: self.input.clone(),
            }))
        }
        fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            self.conn_counter.fetch_add(1, Ordering::Relaxed);
            Some(Box::new(Test1NetworkHandler {
                input: self.input.clone(),
            }))
        }
        fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
        }
        fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
        }
        fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {}
        fn on_event(&mut self, agent: &NetworkAgent<HE, MSG>, event: NetworkBehaviorEvent) {}
        fn on_handler_event(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, event: HE) {}
    }

    impl<BE, MSG> ConnectionHandler<BE, MSG> for Test1NetworkHandler<MSG>
        where BE: From<Behavior1Event> + TryInto<Behavior1Event>,
              MSG: From<Behavior1Msg> + TryInto<Behavior1Msg> + Send + Sync,
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, MSG>) {
            self.input.lock().push_back(
                DebugInput::Opened(
                    agent.remote_peer_id(),
                    agent.conn_id(),
                )
            );
        }
        fn on_tick(&mut self, agent: &ConnectionAgent<BE, MSG>, ts_ms: u64, interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: ConnectionEvent<MSG>) {
            match event {
                ConnectionEvent::Msg { msg, .. } => {
                    match msg {
                        ConnectionMsg::Reliable { data, stream_id } => {
                            if let Ok(msg) = data.try_into() {
                                match msg {
                                    Behavior1Msg::Ping => {
                                        agent.send(ConnectionMsg::Reliable {
                                            stream_id,
                                            data: Behavior1Msg::Pong.into(),
                                        });
                                        self.input.lock().push_back(
                                            DebugInput::Msg(
                                                agent.remote_peer_id(),
                                                agent.conn_id(),
                                                Behavior1Msg::Ping.into()
                                            )
                                        );
                                    }
                                    Behavior1Msg::Pong => {
                                        self.input.lock().push_back(
                                            DebugInput::Msg(
                                                agent.remote_peer_id(),
                                                agent.conn_id(),
                                                Behavior1Msg::Pong.into()
                                            )
                                        );
                                    }
                                }
                            }
                        }
                        ConnectionMsg::Unreliable { data, .. } => {
                            if let Ok(msg) = data.try_into() {

                            }
                        }
                    }
                }
                ConnectionEvent::Stats(_) => {}
            }
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: BE) {}

        fn on_closed(&mut self, agent: &ConnectionAgent<BE, MSG>) {
            self.input.lock().push_back(
                DebugInput::Closed(
                    agent.remote_peer_id(),
                    agent.conn_id(),
                )
            );
        }
    }

    impl<BE, HE, MSG> NetworkBehavior<BE, HE, MSG> for Test2NetworkBehavior
        where BE: From<Behavior2Event> + TryInto<Behavior2Event>,
              HE: From<Handler2Event> + TryInto<Handler2Event>,
              MSG: From<Behavior2Msg> + TryInto<Behavior2Msg> + Send + Sync,
    {
        fn service_id(&self) -> u8 {
            1
        }
        fn on_tick(&mut self, agent: &NetworkAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {}
        fn on_incoming_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }
        fn on_outgoing_connection_connected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) -> Option<Box<dyn ConnectionHandler<BE, MSG>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }
        fn on_incoming_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) {}
        fn on_outgoing_connection_disconnected(&mut self, agent: &NetworkAgent<HE, MSG>, connection: Arc<dyn ConnectionSender<MSG>>) {}
        fn on_outgoing_connection_error(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, err: &OutgoingConnectionError) {}
        fn on_event(&mut self, agent: &NetworkAgent<HE, MSG>, event: NetworkBehaviorEvent) {}
        fn on_handler_event(&mut self, agent: &NetworkAgent<HE, MSG>, peer_id: PeerId, connection_id: u32, event: HE) {}
    }

    impl<BE, MSG> ConnectionHandler<BE, MSG> for Test2NetworkHandler
        where BE: From<Behavior2Event> + TryInto<Behavior2Event>,
              MSG: From<Behavior2Msg> + TryInto<Behavior2Msg>,
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, MSG>) {}

        fn on_tick(&mut self, agent: &ConnectionAgent<BE, MSG>, ts_ms: u64, interal_ms: u64) {}

        fn on_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: ConnectionEvent<MSG>) {}

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, MSG>, event: BE) {}

        fn on_closed(&mut self, agent: &ConnectionAgent<BE, MSG>) {}
    }

    #[async_std::test]
    async fn simple_network_handle() {
        let conn_counter: Arc<AtomicU32> = Default::default();
        let input = Arc::new(Mutex::new(VecDeque::new()));

        let behavior1 = Box::new(Test1NetworkBehavior {
            conn_counter: conn_counter.clone(),
            input: input.clone(),
        });
        let behavior2 = Box::new(Test2NetworkBehavior {});

        let (mock, faker, output) = MockTransport::<ImplNetworkMsg>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let mut plane = NetworkPlane::<ImplNetworkBehaviorEvent, ImplNetworkHandlerEvent, ImplNetworkMsg>::new(NetworkPlaneConfig {
            local_peer_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior1, behavior2],
            transport,
            timer,
        });

        async_std::task::spawn(async move {
           while let Ok(_) = plane.run().await {

           }
        });

        faker.send(MockInput::FakeIncomingConnection(1, 1, "addr1".to_string())).await.unwrap();
        faker.send(MockInput::FakeIncomingMsg(0, 1, ConnectionMsg::Reliable {
            stream_id: 0,
            data: ImplNetworkMsg::Service1(Behavior1Msg::Ping),
        })).await.unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(input.lock().pop_front(), Some(DebugInput::Opened(1, 1)));
        assert_eq!(input.lock().pop_front(), Some(DebugInput::Msg(1, 1, ImplNetworkMsg::Service1(Behavior1Msg::Ping))));
        assert_eq!(conn_counter.load(Ordering::Relaxed), 1);
        assert_eq!(output.lock().pop_front(), Some(MockOutput::SendTo(0, 1, 1, ConnectionMsg::Reliable {
            stream_id: 0,
            data: ImplNetworkMsg::Service1(Behavior1Msg::Pong),
        })));
        faker.send(MockInput::FakeDisconnectIncoming(1, 1)).await.unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 0);
    }
}
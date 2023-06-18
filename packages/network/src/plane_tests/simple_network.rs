#[cfg(test)]
mod tests {
    use crate::behaviour::{ConnectionHandler, NetworkBehavior};
    use crate::mock::{MockInput, MockOutput, MockTransport, MockTransportRpc};
    use crate::plane::{NetworkPlane, NetworkPlaneConfig};
    use crate::router::ForceLocalRouter;
    use crate::transport::{ConnectionEvent, ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
    use crate::{BehaviorAgent, ConnectionAgent};
    use bluesea_identity::{ConnId, NodeAddr, NodeId, Protocol};
    use parking_lot::Mutex;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use utils::SystemTimer;
    use serde::{Serialize, Deserialize};
    use crate::msg::{MsgHeader, MsgRoute, TransportMsg};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    enum Behavior1Msg {
        Ping,
        Pong,
    }
    enum Behavior1Event {}
    enum Handler1Event {}
    #[derive(PartialEq, Debug)]
    enum Behavior1Req {}
    #[derive(PartialEq, Debug)]
    enum Behavior1Res {}

    #[derive(PartialEq, Debug)]
    enum Behavior2Msg {
        Ping,
        Pong,
    }

    #[derive(PartialEq, Debug)]
    enum DebugInput {
        Opened(NodeId, ConnId),
        Msg(NodeId, ConnId, ImplNetworkMsg),
        Closed(NodeId, ConnId),
    }

    enum Behavior2Event {}
    enum Handler2Event {}
    #[derive(PartialEq, Debug)]
    enum Behavior2Req {}
    #[derive(PartialEq, Debug)]
    enum Behavior2Res {}

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplNetworkBehaviorEvent {
        Service1(Behavior1Event),
        Service2(Behavior2Event),
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

    #[derive(PartialEq, Debug, convert_enum::From, convert_enum::TryInto)]
    enum ImplNetworkReq {
        Service1(Behavior1Req),
        Service2(Behavior2Req),
    }

    #[derive(PartialEq, Debug, convert_enum::From, convert_enum::TryInto)]
    enum ImplNetworkRes {
        Service1(Behavior1Res),
        Service2(Behavior2Res),
    }

    struct Test1NetworkBehavior {
        conn_counter: Arc<AtomicU32>,
        input: Arc<Mutex<VecDeque<DebugInput>>>,
    }

    struct Test1NetworkHandler {
        input: Arc<Mutex<VecDeque<DebugInput>>>,
    }

    struct Test2NetworkBehavior {}

    struct Test2NetworkHandler {}

    impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for Test1NetworkBehavior
    where
        BE: From<Behavior1Event> + TryInto<Behavior1Event> + Send + Sync + 'static,
        HE: From<Handler1Event> + TryInto<Handler1Event> + Send + Sync + 'static,
    {
        fn service_id(&self) -> u8 {
            0
        }
        fn on_tick(&mut self, agent: &BehaviorAgent<HE>, ts_ms: u64, interal_ms: u64) {}

        fn check_incoming_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn check_outgoing_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn on_incoming_connection_connected(&mut self, agent: &BehaviorAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            self.conn_counter.fetch_add(1, Ordering::SeqCst);
            Some(Box::new(Test1NetworkHandler { input: self.input.clone() }))
        }
        fn on_outgoing_connection_connected(&mut self, agent: &BehaviorAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            self.conn_counter.fetch_add(1, Ordering::SeqCst);
            Some(Box::new(Test1NetworkHandler { input: self.input.clone() }))
        }
        fn on_incoming_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) {
            self.conn_counter.fetch_sub(1, Ordering::SeqCst);
        }
        fn on_outgoing_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) {
            self.conn_counter.fetch_sub(1, Ordering::SeqCst);
        }
        fn on_outgoing_connection_error(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError) {}
        fn on_handler_event(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, event: BE) {}

        fn on_rpc(&mut self, agent: &BehaviorAgent<HE>, req: Req, res: Box<dyn RpcAnswer<Res>>) -> bool {
            todo!()
        }
    }

    impl<BE, HE> ConnectionHandler<BE, HE> for Test1NetworkHandler
    where
        BE: From<Behavior1Event> + TryInto<Behavior1Event> + Send + Sync + 'static,
        HE: From<Handler1Event> + TryInto<Handler1Event> + Send + Sync + 'static,
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE>) {
            self.input.lock().push_back(DebugInput::Opened(agent.remote_node_id(), agent.conn_id()));
        }
        fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE>, ts_ms: u64, interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {
            match event {
                ConnectionEvent::Msg(msg) => {
                    if let Ok(msg) = msg.get_payload_bincode::<Behavior1Msg>() {
                        match msg {
                            Behavior1Msg::Ping => {
                                agent.send_net( TransportMsg::from_payload_bincode(MsgHeader::build_simple(0, MsgRoute::ToNode(1), 0), &Behavior1Msg::Pong).unwrap());
                                self.input.lock().push_back(DebugInput::Msg(agent.remote_node_id(), agent.conn_id(), Behavior1Msg::Ping.into()));
                            }
                            Behavior1Msg::Pong => {
                                self.input.lock().push_back(DebugInput::Msg(agent.remote_node_id(), agent.conn_id(), Behavior1Msg::Pong.into()));
                            }
                        }
                    }
                },
                ConnectionEvent::Stats(_) => {}
            }
        }

        fn on_other_handler_event(&mut self, agent: &ConnectionAgent<BE, HE>, from_node: NodeId, from_conn: ConnId, event: HE) {}

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: HE) {}

        fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE>) {
            self.input.lock().push_back(DebugInput::Closed(agent.remote_node_id(), agent.conn_id()));
        }
    }

    impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for Test2NetworkBehavior
    where
        BE: From<Behavior2Event> + TryInto<Behavior2Event>,
        HE: From<Handler2Event> + TryInto<Handler2Event>,
    {
        fn service_id(&self) -> u8 {
            1
        }
        fn on_tick(&mut self, agent: &BehaviorAgent<HE>, ts_ms: u64, interal_ms: u64) {}

        fn check_incoming_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn check_outgoing_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn on_incoming_connection_connected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }
        fn on_outgoing_connection_connected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            Some(Box::new(Test2NetworkHandler {}))
        }
        fn on_incoming_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) {}
        fn on_outgoing_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) {}
        fn on_outgoing_connection_error(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError) {}
        fn on_handler_event(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, event: BE) {}

        fn on_rpc(&mut self, agent: &BehaviorAgent<HE>, req: Req, res: Box<dyn RpcAnswer<Res>>) -> bool {
            todo!()
        }
    }

    impl<BE, HE> ConnectionHandler<BE, HE> for Test2NetworkHandler
    where
        BE: From<Behavior2Event> + TryInto<Behavior2Event>,
        HE: From<Handler2Event> + TryInto<Handler2Event>,
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE>) {}
        fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE>, ts_ms: u64, interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {}
        fn on_other_handler_event(&mut self, agent: &ConnectionAgent<BE, HE>, from_node: NodeId, from_conn: ConnId, event: HE) {}
        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: HE) {}
        fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE>) {}
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

        let (mock, faker, output) = MockTransport::new();
        let (mock_rpc, faker_rpc, output_rpc) = MockTransportRpc::<ImplNetworkReq, ImplNetworkRes>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let mut plane = NetworkPlane::<ImplNetworkBehaviorEvent, ImplNetworkHandlerEvent, ImplNetworkReq, ImplNetworkRes>::new(NetworkPlaneConfig {
            local_node_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior1, behavior2],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
            router: Arc::new(ForceLocalRouter()),
        });

        async_std::task::spawn(async move { while let Ok(_) = plane.recv().await {} });

        faker.send(MockInput::FakeIncomingConnection(1, ConnId::from_in(0, 1), NodeAddr::from(Protocol::Udp(1)))).await.unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        faker
            .send(MockInput::FakeIncomingMsg(
                ConnId::from_in(0, 1),
                TransportMsg::from_payload_bincode(MsgHeader::build_simple(0, MsgRoute::ToNode(1), 0), &Behavior1Msg::Ping).unwrap(),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(input.lock().pop_front(), Some(DebugInput::Opened(1, ConnId::from_in(0, 1))));
        assert_eq!(input.lock().pop_front(), Some(DebugInput::Msg(1, ConnId::from_in(0, 1), ImplNetworkMsg::Service1(Behavior1Msg::Ping))));
        assert_eq!(conn_counter.load(Ordering::SeqCst), 1);
        assert_eq!(
            output.lock().pop_front(),
            Some(MockOutput::SendTo(
                1,
                ConnId::from_in(0, 1),
                TransportMsg::from_payload_bincode(MsgHeader::build_simple(0, MsgRoute::ToNode(1), 0), &Behavior1Msg::Pong).unwrap(),
            ))
        );
        faker.send(MockInput::FakeDisconnectIncoming(1, ConnId::from_in(0, 1))).await.unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(conn_counter.load(Ordering::SeqCst), 0);
    }
}

#[cfg(test)]
mod tests {
    use crate::behaviour::{ConnectionHandler, NetworkBehavior};
    use crate::mock::{MockInput, MockTransport, MockTransportRpc};
    use crate::msg::{MsgHeader, TransportMsg};
    use crate::plane::{NetworkPlane, NetworkPlaneConfig};
    use crate::transport::{ConnectionEvent, ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
    use crate::{BehaviorAgent, ConnectionAgent, CrossHandlerRoute};
    use bluesea_identity::{ConnId, NodeAddr, NodeId, Protocol};
    use bluesea_router::{ForceLocalRouter, RouteRule};
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use utils::option_handle::OptionUtils;
    use utils::SystemTimer;

    #[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
    enum TestCrossNetworkMsg {
        PingToNode(NodeId),
        PingToConn(ConnId),
    }
    enum TestCrossBehaviorEvent {}
    enum TestCrossHandleEvent {
        Ping,
        Pong,
    }
    enum TestCrossNetworkReq {}
    enum TestCrossNetworkRes {}

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplTestCrossNetworkBehaviorEvent {
        Test(TestCrossBehaviorEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplTestCrossNetworkHandlerEvent {
        Test(TestCrossHandleEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplTestCrossNetworkMsg {
        Test(TestCrossNetworkMsg),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplTestCrossNetworkReq {
        Test(TestCrossNetworkReq),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplTestCrossNetworkRes {
        Test(TestCrossNetworkRes),
    }

    struct TestCrossNetworkBehavior {
        flag: Arc<AtomicBool>,
    }
    struct TestCrossNetworkHandler {
        flag: Arc<AtomicBool>,
    }

    impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for TestCrossNetworkBehavior
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
    {
        fn service_id(&self) -> u8 {
            0
        }
        fn on_tick(&mut self, _agent: &BehaviorAgent<BE, HE>, _ts_ms: u64, _interal_ms: u64) {}

        fn check_incoming_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn check_outgoing_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn on_local_msg(&mut self, _agent: &BehaviorAgent<BE, HE>, _msg: TransportMsg) {}

        fn on_incoming_connection_connected(&mut self, _agent: &BehaviorAgent<BE, HE>, _connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            Some(Box::new(TestCrossNetworkHandler { flag: self.flag.clone() }))
        }
        fn on_outgoing_connection_connected(&mut self, _agent: &BehaviorAgent<BE, HE>, _connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            Some(Box::new(TestCrossNetworkHandler { flag: self.flag.clone() }))
        }
        fn on_incoming_connection_disconnected(&mut self, _agent: &BehaviorAgent<BE, HE>, _connection: Arc<dyn ConnectionSender>) {}
        fn on_outgoing_connection_disconnected(&mut self, _agent: &BehaviorAgent<BE, HE>, _connection: Arc<dyn ConnectionSender>) {}
        fn on_outgoing_connection_error(&mut self, _agent: &BehaviorAgent<BE, HE>, _node_id: NodeId, _conn_id: ConnId, _err: &OutgoingConnectionError) {}
        fn on_handler_event(&mut self, _agent: &BehaviorAgent<BE, HE>, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

        fn on_rpc(&mut self, _agent: &BehaviorAgent<BE, HE>, _req: Req, _res: Box<dyn RpcAnswer<Res>>) -> bool {
            false
        }
        fn on_started(&mut self, _agent: &BehaviorAgent<BE, HE>) {}

        fn on_stopped(&mut self, _agent: &BehaviorAgent<BE, HE>) {}
    }

    impl<BE, HE> ConnectionHandler<BE, HE> for TestCrossNetworkHandler
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
    {
        fn on_opened(&mut self, _agent: &ConnectionAgent<BE, HE>) {}
        fn on_tick(&mut self, _agent: &ConnectionAgent<BE, HE>, _ts_ms: u64, _interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {
            match event {
                ConnectionEvent::Msg(msg) => {
                    if let Ok(e) = msg.get_payload_bincode::<TestCrossNetworkMsg>() {
                        match e {
                            TestCrossNetworkMsg::PingToNode(node) => {
                                agent.send_to_handler(CrossHandlerRoute::NodeFirst(node), TestCrossHandleEvent::Ping.into());
                            }
                            TestCrossNetworkMsg::PingToConn(conn) => {
                                agent.send_to_handler(CrossHandlerRoute::Conn(conn), TestCrossHandleEvent::Ping.into());
                            }
                        }
                    }
                }
                ConnectionEvent::Stats(_) => {}
            }
        }

        fn on_other_handler_event(&mut self, agent: &ConnectionAgent<BE, HE>, _from_node: NodeId, from_conn: ConnId, event: HE) {
            if let Ok(event) = event.try_into() {
                match event {
                    TestCrossHandleEvent::Ping => {
                        agent.send_to_handler(CrossHandlerRoute::Conn(from_conn), TestCrossHandleEvent::Pong.into());
                    }
                    TestCrossHandleEvent::Pong => {
                        self.flag.store(true, Ordering::Relaxed);
                    }
                }
            }
        }

        fn on_behavior_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _event: HE) {}
        fn on_closed(&mut self, _agent: &ConnectionAgent<BE, HE>) {}
    }

    #[async_std::test]
    async fn test_cross_behaviour_handler_conn() {
        let flag = Arc::new(AtomicBool::new(false));
        let behavior = Box::new(TestCrossNetworkBehavior { flag: flag.clone() });

        let (mock, faker, _output) = MockTransport::new();
        let (mock_rpc, _faker_rpc, _output_rpc) = MockTransportRpc::<ImplTestCrossNetworkReq, ImplTestCrossNetworkRes>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let mut plane = NetworkPlane::<ImplTestCrossNetworkBehaviorEvent, ImplTestCrossNetworkHandlerEvent, ImplTestCrossNetworkReq, ImplTestCrossNetworkRes>::new(NetworkPlaneConfig {
            local_node_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
            router: Arc::new(ForceLocalRouter()),
        });

        let join = async_std::task::spawn(async move {
            plane.started();
            while let Ok(_) = plane.recv().await {}
            plane.stopped();
        });

        faker.send(MockInput::FakeIncomingConnection(1, ConnId::from_in(0, 1), NodeAddr::from(Protocol::Udp(1)))).await.unwrap();
        faker.send(MockInput::FakeIncomingConnection(2, ConnId::from_in(0, 2), NodeAddr::from(Protocol::Udp(2)))).await.unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        faker
            .send(MockInput::FakeIncomingMsg(
                ConnId::from_in(0, 1),
                TransportMsg::from_payload_bincode(MsgHeader::build_reliable(0, RouteRule::ToNode(1), 0), &TestCrossNetworkMsg::PingToConn(ConnId::from_in(0, 2))),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(flag.load(Ordering::Relaxed), true);
        join.cancel().await.print_none("Should cancel join");
    }

    #[async_std::test]
    async fn test_cross_behaviour_handler_node() {
        let flag = Arc::new(AtomicBool::new(false));
        let behavior = Box::new(TestCrossNetworkBehavior { flag: flag.clone() });

        let (mock, faker, _output) = MockTransport::new();
        let (mock_rpc, _faker_rpc, _output_rpc) = MockTransportRpc::<ImplTestCrossNetworkReq, ImplTestCrossNetworkRes>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let mut plane = NetworkPlane::<ImplTestCrossNetworkBehaviorEvent, ImplTestCrossNetworkHandlerEvent, ImplTestCrossNetworkReq, ImplTestCrossNetworkRes>::new(NetworkPlaneConfig {
            local_node_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
            router: Arc::new(ForceLocalRouter()),
        });

        let join = async_std::task::spawn(async move {
            plane.started();
            while let Ok(_) = plane.recv().await {}
            plane.stopped();
        });

        faker.send(MockInput::FakeIncomingConnection(1, ConnId::from_in(0, 1), NodeAddr::from(Protocol::Udp(1)))).await.unwrap();
        faker.send(MockInput::FakeIncomingConnection(2, ConnId::from_in(0, 2), NodeAddr::from(Protocol::Udp(2)))).await.unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        faker
            .send(MockInput::FakeIncomingMsg(
                ConnId::from_in(0, 1),
                TransportMsg::from_payload_bincode(MsgHeader::build_reliable(0, RouteRule::ToNode(1), 0), &TestCrossNetworkMsg::PingToNode(2)),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(flag.load(Ordering::Relaxed), true);
        join.cancel().await.print_none("Should cancel join");
    }
}

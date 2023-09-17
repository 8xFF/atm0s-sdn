#[cfg(test)]
mod tests {
    use crate::behaviour::{ConnectionHandler, NetworkBehavior};
    use crate::mock::{MockInput, MockTransport, MockTransportRpc};
    use crate::msg::{MsgHeader, TransportMsg};
    use crate::plane::{NetworkPlane, NetworkPlaneConfig};
    use crate::transport::{ConnectionEvent, ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
    use crate::{BehaviorAgent, ConnectionAgent};
    use bluesea_identity::{ConnId, NodeAddr, NodeId, Protocol};
    use bluesea_router::{ForceLocalRouter, RouteRule};
    use serde::{Deserialize, Serialize};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use utils::option_handle::OptionUtils;
    use utils::SystemTimer;

    #[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
    enum TestCrossNetworkMsg {
        CloseInHandle,
        CloseInBehaviorConn,
        CloseInBehaviorNode,
    }
    enum TestCrossBehaviorEvent {
        CloseConn(ConnId),
        CloseNode(NodeId),
    }
    enum TestCrossHandleEvent {}
    enum TestCrossBehaviorReq {}
    enum TestCrossBehaviorRes {}

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
        Test(TestCrossBehaviorReq),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplTestCrossNetworkRes {
        Test(TestCrossBehaviorRes),
    }

    struct TestCrossNetworkBehavior {
        conn_counter: Arc<AtomicU32>,
    }
    struct TestCrossNetworkHandler {
        conn_counter: Arc<AtomicU32>,
    }

    impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for TestCrossNetworkBehavior
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
    {
        fn service_id(&self) -> u8 {
            0
        }
        fn on_tick(&mut self, _agent: &BehaviorAgent<HE>, _ts_ms: u64, _interal_ms: u64) {}

        fn check_incoming_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn check_outgoing_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn on_incoming_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, _connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            self.conn_counter.fetch_add(1, Ordering::Relaxed);
            Some(Box::new(TestCrossNetworkHandler {
                conn_counter: self.conn_counter.clone(),
            }))
        }
        fn on_outgoing_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, _connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
            self.conn_counter.fetch_add(1, Ordering::Relaxed);
            Some(Box::new(TestCrossNetworkHandler {
                conn_counter: self.conn_counter.clone(),
            }))
        }
        fn on_incoming_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, _conn: Arc<dyn ConnectionSender>) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
        }
        fn on_outgoing_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, _connection: Arc<dyn ConnectionSender>) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
        }
        fn on_outgoing_connection_error(&mut self, _agent: &BehaviorAgent<HE>, _node_id: NodeId, _conn_id: ConnId, _err: &OutgoingConnectionError) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
        }
        fn on_handler_event(&mut self, agent: &BehaviorAgent<HE>, _node_id: NodeId, _conn_id: ConnId, event: BE) {
            if let Ok(event) = event.try_into() {
                match event {
                    TestCrossBehaviorEvent::CloseConn(conn) => {
                        agent.close_conn(conn);
                    }
                    TestCrossBehaviorEvent::CloseNode(node_id) => {
                        agent.close_node(node_id);
                    }
                }
            }
        }

        fn on_rpc(&mut self, _agent: &BehaviorAgent<HE>, _req: Req, _res: Box<dyn RpcAnswer<Res>>) -> bool {
            false
        }
    }

    impl<BE, HE> ConnectionHandler<BE, HE> for TestCrossNetworkHandler
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
    {
        fn on_opened(&mut self, _agent: &ConnectionAgent<BE, HE>) {
            self.conn_counter.fetch_add(1, Ordering::Relaxed);
        }
        fn on_tick(&mut self, _agent: &ConnectionAgent<BE, HE>, _ts_ms: u64, _interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, HE>, event: ConnectionEvent) {
            match event {
                ConnectionEvent::Msg(msg) => {
                    if let Ok(e) = msg.get_payload_bincode::<TestCrossNetworkMsg>() {
                        match e {
                            TestCrossNetworkMsg::CloseInBehaviorConn => {
                                agent.send_behavior(TestCrossBehaviorEvent::CloseConn(agent.conn_id()).into());
                            }
                            TestCrossNetworkMsg::CloseInBehaviorNode => {
                                agent.send_behavior(TestCrossBehaviorEvent::CloseNode(agent.remote_node_id()).into());
                            }
                            TestCrossNetworkMsg::CloseInHandle => {
                                agent.close_conn();
                            }
                        }
                    }
                }
                ConnectionEvent::Stats(_) => {}
            }
        }

        fn on_other_handler_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _from_node: NodeId, _from_conn: ConnId, _event: HE) {}

        fn on_behavior_event(&mut self, _agent: &ConnectionAgent<BE, HE>, _event: HE) {}
        fn on_closed(&mut self, _agent: &ConnectionAgent<BE, HE>) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
        }
    }

    #[async_std::test]
    async fn test_behaviour_close_conn() {
        let conn_counter: Arc<AtomicU32> = Default::default();
        let behavior = Box::new(TestCrossNetworkBehavior { conn_counter: conn_counter.clone() });

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

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.recv().await {} });

        faker.send(MockInput::FakeIncomingConnection(1, ConnId::from_in(0, 1), NodeAddr::from(Protocol::Udp(1)))).await.unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 1);
        faker
            .send(MockInput::FakeIncomingMsg(
                ConnId::from_in(0, 1),
                TransportMsg::from_payload_bincode(MsgHeader::build_reliable(0, RouteRule::ToNode(1), 0), &TestCrossNetworkMsg::CloseInHandle).unwrap(),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 0);

        faker.send(MockInput::FakeIncomingConnection(1, ConnId::from_in(0, 2), NodeAddr::from(Protocol::Udp(1)))).await.unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 1);
        faker
            .send(MockInput::FakeIncomingMsg(
                ConnId::from_in(0, 2),
                TransportMsg::from_payload_bincode(MsgHeader::build_reliable(0, RouteRule::ToNode(1), 0), &TestCrossNetworkMsg::CloseInBehaviorConn).unwrap(),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 0);

        faker.send(MockInput::FakeIncomingConnection(1, ConnId::from_in(0, 3), NodeAddr::from(Protocol::Udp(1)))).await.unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 1);
        faker
            .send(MockInput::FakeIncomingMsg(
                ConnId::from_in(0, 3),
                TransportMsg::from_payload_bincode(MsgHeader::build_reliable(0, RouteRule::ToNode(1), 0), &TestCrossNetworkMsg::CloseInBehaviorNode).unwrap(),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 0);

        join.cancel().await.print_none("Should can join");
    }
}

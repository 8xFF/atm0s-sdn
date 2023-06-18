#[cfg(test)]
mod tests {
    use crate::behaviour::{ConnectionHandler, NetworkBehavior};
    use crate::mock::{MockInput, MockOutput, MockTransport, MockTransportRpc};
    use crate::plane::{NetworkPlane, NetworkPlaneConfig};
    use crate::transport::{
        ConnectionEvent, ConnectionMsg, ConnectionRejectReason, ConnectionSender,
        OutgoingConnectionError, RpcAnswer,
    };
    use crate::{BehaviorAgent, ConnectionAgent, CrossHandlerRoute};
    use bluesea_identity::{ConnId, NodeAddr, NodeId, Protocol};
    use parking_lot::Mutex;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use utils::SystemTimer;
    use crate::router::ForceLocalRouter;

    enum TestCrossNetworkMsg {}
    enum TestCrossBehaviorEvent {
        Ping,
    }
    enum TestCrossHandleEvent {
        Pong,
    }

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
    enum ImplTestCrossNetworkReq {}

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplTestCrossNetworkRes {}

    struct TestCrossNetworkBehavior {
        flag: Arc<AtomicBool>,
    }
    struct TestCrossNetworkHandler {
        flag: Arc<AtomicBool>,
    }

    impl<BE, HE, MSG, Req, Res> NetworkBehavior<BE, HE, MSG, Req, Res> for TestCrossNetworkBehavior
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
        MSG: From<TestCrossNetworkMsg> + TryInto<TestCrossNetworkMsg> + Send + Sync + 'static,
    {
        fn service_id(&self) -> u8 {
            1
        }
        fn on_tick(&mut self, agent: &BehaviorAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {}

        fn check_incoming_connection(
            &mut self,
            node: NodeId,
            conn_id: ConnId,
        ) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn check_outgoing_connection(
            &mut self,
            node: NodeId,
            conn_id: ConnId,
        ) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn on_incoming_connection_connected(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            connection: Arc<dyn ConnectionSender<MSG>>,
        ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>> {
            Some(Box::new(TestCrossNetworkHandler {
                flag: self.flag.clone(),
            }))
        }
        fn on_outgoing_connection_connected(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            connection: Arc<dyn ConnectionSender<MSG>>,
        ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>> {
            Some(Box::new(TestCrossNetworkHandler {
                flag: self.flag.clone(),
            }))
        }
        fn on_incoming_connection_disconnected(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            connection: Arc<dyn ConnectionSender<MSG>>,
        ) {
        }
        fn on_outgoing_connection_disconnected(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            connection: Arc<dyn ConnectionSender<MSG>>,
        ) {
        }
        fn on_outgoing_connection_error(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            node_id: NodeId,
            conn_id: ConnId,
            err: &OutgoingConnectionError,
        ) {
        }
        fn on_handler_event(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            node_id: NodeId,
            conn_id: ConnId,
            event: BE,
        ) {
            if let Ok(event) = event.try_into() {
                match event {
                    TestCrossBehaviorEvent::Ping => {
                        agent.send_to_handler(
                            CrossHandlerRoute::Conn(conn_id),
                            TestCrossHandleEvent::Pong.into(),
                        );
                    }
                }
            }
        }

        fn on_rpc(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            req: Req,
            res: Box<dyn RpcAnswer<Res>>,
        ) -> bool {
            todo!()
        }
    }

    impl<BE, HE, MSG> ConnectionHandler<BE, HE, MSG> for TestCrossNetworkHandler
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
        MSG: From<TestCrossNetworkMsg> + TryInto<TestCrossNetworkMsg> + Send + Sync + 'static,
    {
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {
            agent.send_behavior(TestCrossBehaviorEvent::Ping.into());
        }
        fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>) {}

        fn on_other_handler_event(
            &mut self,
            agent: &ConnectionAgent<BE, HE, MSG>,
            from_node: NodeId,
            from_conn: ConnId,
            event: HE,
        ) {
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE) {
            if let Ok(event) = event.try_into() {
                match event {
                    TestCrossHandleEvent::Pong => {
                        self.flag.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
        fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {}
    }

    #[async_std::test]
    async fn test_cross_behaviour_handler() {
        let flag = Arc::new(AtomicBool::new(false));
        let behavior = Box::new(TestCrossNetworkBehavior { flag: flag.clone() });

        let (mock, faker, output) = MockTransport::<ImplTestCrossNetworkMsg>::new();
        let (mock_rpc, faker_rpc, output_rpc) =
            MockTransportRpc::<ImplTestCrossNetworkReq, ImplTestCrossNetworkRes>::new();
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let mut plane = NetworkPlane::<
            ImplTestCrossNetworkBehaviorEvent,
            ImplTestCrossNetworkHandlerEvent,
            ImplTestCrossNetworkMsg,
            ImplTestCrossNetworkReq,
            ImplTestCrossNetworkRes,
        >::new(NetworkPlaneConfig {
            local_node_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
            router: Arc::new(ForceLocalRouter()),
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.recv().await {} });

        faker
            .send(MockInput::FakeIncomingConnection(
                1,
                ConnId::from_in(0, 1),
                NodeAddr::from(Protocol::Udp(1)),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(flag.load(Ordering::Relaxed), true);
        join.cancel();
    }
}

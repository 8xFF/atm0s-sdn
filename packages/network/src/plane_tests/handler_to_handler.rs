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
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::{PeerAddr, PeerId};
    use parking_lot::Mutex;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use utils::SystemTimer;

    enum TestCrossNetworkMsg {
        PingToPeer(PeerId),
        PingToConn(u32),
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

    impl<BE, HE, MSG, Req, Res> NetworkBehavior<BE, HE, MSG, Req, Res> for TestCrossNetworkBehavior
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
        MSG: From<TestCrossNetworkMsg> + TryInto<TestCrossNetworkMsg> + Send + Sync + 'static,
    {
        fn service_id(&self) -> u8 {
            0
        }
        fn on_tick(&mut self, agent: &BehaviorAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {}

        fn check_incoming_connection(
            &mut self,
            peer: PeerId,
            conn_id: u32,
        ) -> Result<(), ConnectionRejectReason> {
            Ok(())
        }

        fn check_outgoing_connection(
            &mut self,
            peer: PeerId,
            conn_id: u32,
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
            peer_id: PeerId,
            connection_id: u32,
            err: &OutgoingConnectionError,
        ) {
        }
        fn on_handler_event(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            peer_id: PeerId,
            connection_id: u32,
            event: BE,
        ) {
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
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {}
        fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>) {
            match event {
                ConnectionEvent::Msg { service_id, msg } => match msg {
                    ConnectionMsg::Reliable { data, .. } => {
                        if let Ok(e) = data.try_into() {
                            match e {
                                TestCrossNetworkMsg::PingToPeer(peer) => {
                                    agent.send_to_handler(
                                        CrossHandlerRoute::PeerFirst(peer),
                                        TestCrossHandleEvent::Ping.into(),
                                    );
                                }
                                TestCrossNetworkMsg::PingToConn(conn) => {
                                    agent.send_to_handler(
                                        CrossHandlerRoute::Conn(conn),
                                        TestCrossHandleEvent::Ping.into(),
                                    );
                                }
                            }
                        }
                    }
                    ConnectionMsg::Unreliable { .. } => {}
                },
                ConnectionEvent::Stats(_) => {}
            }
        }

        fn on_other_handler_event(
            &mut self,
            agent: &ConnectionAgent<BE, HE, MSG>,
            from_peer: PeerId,
            from_conn: u32,
            event: HE,
        ) {
            if let Ok(event) = event.try_into() {
                match event {
                    TestCrossHandleEvent::Ping => {
                        agent.send_to_handler(
                            CrossHandlerRoute::Conn(from_conn),
                            TestCrossHandleEvent::Pong.into(),
                        );
                    }
                    TestCrossHandleEvent::Pong => {
                        self.flag.store(true, Ordering::Relaxed);
                    }
                }
            }
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE) {}
        fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {}
    }

    #[async_std::test]
    async fn test_cross_behaviour_handler_conn() {
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
            local_peer_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });

        faker
            .send(MockInput::FakeIncomingConnection(
                1,
                1,
                PeerAddr::from(Protocol::Udp(1)),
            ))
            .await
            .unwrap();
        faker
            .send(MockInput::FakeIncomingConnection(
                2,
                2,
                PeerAddr::from(Protocol::Udp(2)),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        faker
            .send(MockInput::FakeIncomingMsg(
                0,
                1,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: TestCrossNetworkMsg::PingToConn(2).into(),
                },
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(flag.load(Ordering::Relaxed), true);
        join.cancel();
    }

    #[async_std::test]
    async fn test_cross_behaviour_handler_peer() {
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
            local_peer_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior],
            transport,
            transport_rpc: Box::new(mock_rpc),
            timer,
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });

        faker
            .send(MockInput::FakeIncomingConnection(
                1,
                1,
                PeerAddr::from(Protocol::Udp(1)),
            ))
            .await
            .unwrap();
        faker
            .send(MockInput::FakeIncomingConnection(
                2,
                2,
                PeerAddr::from(Protocol::Udp(2)),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        faker
            .send(MockInput::FakeIncomingMsg(
                0,
                1,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: TestCrossNetworkMsg::PingToPeer(2).into(),
                },
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(flag.load(Ordering::Relaxed), true);
        join.cancel();
    }
}

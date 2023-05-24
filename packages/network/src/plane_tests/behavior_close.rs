#[cfg(test)]
mod tests {
    use crate::behaviour::{ConnectionHandler, NetworkBehavior};
    use crate::mock::{MockInput, MockOutput, MockTransport, MockTransportRpc};
    use crate::plane::{NetworkPlane, NetworkPlaneConfig};
    use crate::transport::{
        ConnectionEvent, ConnectionMsg, ConnectionRejectReason, ConnectionSender,
        OutgoingConnectionError, RpcAnswer,
    };
    use crate::{BehaviorAgent, ConnectionAgent};
    use bluesea_identity::multiaddr::Protocol;
    use bluesea_identity::{PeerAddr, PeerId};
    use parking_lot::Mutex;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use utils::SystemTimer;

    enum TestCrossNetworkMsg {
        CloseInHandle,
        CloseInBehaviorConn,
        CloseInBehaviorPeer,
    }
    enum TestCrossBehaviorEvent {
        CloseConn(u32),
        ClosePeer(PeerId),
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
            self.conn_counter.fetch_add(1, Ordering::Relaxed);
            Some(Box::new(TestCrossNetworkHandler {
                conn_counter: self.conn_counter.clone(),
            }))
        }
        fn on_outgoing_connection_connected(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            connection: Arc<dyn ConnectionSender<MSG>>,
        ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>> {
            self.conn_counter.fetch_add(1, Ordering::Relaxed);
            Some(Box::new(TestCrossNetworkHandler {
                conn_counter: self.conn_counter.clone(),
            }))
        }
        fn on_incoming_connection_disconnected(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            connection: Arc<dyn ConnectionSender<MSG>>,
        ) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
        }
        fn on_outgoing_connection_disconnected(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            connection: Arc<dyn ConnectionSender<MSG>>,
        ) {
            self.conn_counter.fetch_sub(1, Ordering::Relaxed);
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
            if let Ok(event) = event.try_into() {
                match event {
                    TestCrossBehaviorEvent::CloseConn(conn) => {
                        agent.close_conn(conn);
                    }
                    TestCrossBehaviorEvent::ClosePeer(peer_id) => {
                        agent.close_peer(peer_id);
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
        fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {}
        fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, ts_ms: u64, interal_ms: u64) {}
        fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: ConnectionEvent<MSG>) {
            match event {
                ConnectionEvent::Msg { service_id, msg } => match msg {
                    ConnectionMsg::Reliable { data, .. } => {
                        if let Ok(e) = data.try_into() {
                            match e {
                                TestCrossNetworkMsg::CloseInBehaviorConn => {
                                    agent.send_behavior(
                                        TestCrossBehaviorEvent::CloseConn(agent.conn_id()).into(),
                                    );
                                }
                                TestCrossNetworkMsg::CloseInBehaviorPeer => {
                                    agent.send_behavior(
                                        TestCrossBehaviorEvent::ClosePeer(agent.remote_peer_id())
                                            .into(),
                                    );
                                }
                                TestCrossNetworkMsg::CloseInHandle => {
                                    agent.close_conn();
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
        }

        fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, MSG>, event: HE) {}
        fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, MSG>) {}
    }

    #[async_std::test]
    async fn test_behaviour_close_conn() {
        let conn_counter: Arc<AtomicU32> = Default::default();
        let behavior = Box::new(TestCrossNetworkBehavior {
            conn_counter: conn_counter.clone(),
        });

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
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 1);
        faker
            .send(MockInput::FakeIncomingMsg(
                0,
                1,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: TestCrossNetworkMsg::CloseInHandle.into(),
                },
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 0);

        faker
            .send(MockInput::FakeIncomingConnection(
                1,
                2,
                PeerAddr::from(Protocol::Udp(1)),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 1);
        faker
            .send(MockInput::FakeIncomingMsg(
                0,
                2,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: TestCrossNetworkMsg::CloseInBehaviorConn.into(),
                },
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 0);

        faker
            .send(MockInput::FakeIncomingConnection(
                1,
                3,
                PeerAddr::from(Protocol::Udp(1)),
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 1);
        faker
            .send(MockInput::FakeIncomingMsg(
                0,
                3,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: TestCrossNetworkMsg::CloseInBehaviorPeer.into(),
                },
            ))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(conn_counter.load(Ordering::Relaxed), 0);

        join.cancel();
    }
}

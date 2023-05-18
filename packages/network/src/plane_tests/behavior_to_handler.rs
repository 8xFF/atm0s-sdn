#[cfg(test)]
mod tests {
    use crate::behaviour::{ConnectionHandler, NetworkBehavior};
    use crate::mock::{MockInput, MockOutput, MockTransport};
    use crate::plane::{
        BehaviorAgent, ConnectionAgent, CrossHandlerRoute, NetworkPlane, NetworkPlaneConfig,
    };
    use crate::transport::{
        ConnectionEvent, ConnectionMsg, ConnectionSender, OutgoingConnectionError,
    };
    use bluesea_identity::{PeerAddr, PeerId};
    use parking_lot::Mutex;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use bluesea_identity::multiaddr::Protocol;
    use utils::SystemTimer;

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

    struct TestCrossNetworkBehavior {
        flag: Arc<AtomicBool>,
    }
    struct TestCrossNetworkHandler {
        flag: Arc<AtomicBool>,
    }

    impl<BE, HE, MSG> NetworkBehavior<BE, HE, MSG> for TestCrossNetworkBehavior
    where
        BE: From<TestCrossBehaviorEvent> + TryInto<TestCrossBehaviorEvent> + Send + Sync + 'static,
        HE: From<TestCrossHandleEvent> + TryInto<TestCrossHandleEvent> + Send + Sync + 'static,
        MSG: From<TestCrossNetworkMsg> + TryInto<TestCrossNetworkMsg> + Send + Sync + 'static,
    {
        fn service_id(&self) -> u8 {
            1
        }
        fn on_tick(&mut self, agent: &BehaviorAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {}
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
        ) {}
        fn on_handler_event(
            &mut self,
            agent: &BehaviorAgent<HE, MSG>,
            peer_id: PeerId,
            connection_id: u32,
            event: BE,
        ) {
            if let Ok(event) = event.try_into() {
                match event {
                    TestCrossBehaviorEvent::Ping => {
                        agent.send_to_handler(
                            CrossHandlerRoute::Conn(connection_id),
                            TestCrossHandleEvent::Pong.into(),
                        );
                    }
                }
            }
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
            from_peer: PeerId,
            from_conn: u32,
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
        let transport = Box::new(mock);
        let timer = Arc::new(SystemTimer());

        let mut plane = NetworkPlane::<
            ImplTestCrossNetworkBehaviorEvent,
            ImplTestCrossNetworkHandlerEvent,
            ImplTestCrossNetworkMsg,
        >::new(NetworkPlaneConfig {
            local_peer_id: 0,
            tick_ms: 1000,
            behavior: vec![behavior],
            transport,
            timer,
        });

        let join = async_std::task::spawn(async move { while let Ok(_) = plane.run().await {} });

        faker
            .send(MockInput::FakeIncomingConnection(1, 1, PeerAddr::from(Protocol::Udp(1))))
            .await
            .unwrap();
        async_std::task::sleep(Duration::from_millis(1000)).await;
        assert_eq!(flag.load(Ordering::Relaxed), true);
        join.cancel();
    }
}

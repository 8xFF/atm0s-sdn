#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use async_std::{prelude::FutureExt, task::JoinHandle};
    use atm0s_sdn::{
        convert_enum, KeyValueBehavior, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent, LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent,
        LayersSpreadRouterSyncHandlerEvent, ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, NetworkPlane, NetworkPlaneConfig, NodeAddr, NodeAddrBuilder, NodeId, Protocol,
        RouteRule, RpcBox, RpcMsg, RpcMsgParam, SharedRouter, SystemTimer,
    };
    use atm0s_sdn_transport_vnet::VnetEarth;

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum BE {
        KeyValue(KeyValueBehaviorEvent),
        RouterSync(LayersSpreadRouterSyncBehaviorEvent),
        Manual(ManualBehaviorEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum HE {
        KeyValue(KeyValueHandlerEvent),
        RouterSync(LayersSpreadRouterSyncHandlerEvent),
        Manual(ManualHandlerEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum SE {
        KeyValue(KeyValueSdkEvent),
    }

    async fn run_node(vnet: Arc<VnetEarth>, rpc_service_id: u8, node_id: NodeId, seeds: Vec<NodeAddr>) -> (RpcBox, NodeAddr, JoinHandle<()>) {
        log::info!("Run node {} connect to {:?}", node_id, seeds);
        let node_addr = Arc::new(NodeAddrBuilder::default());
        node_addr.add_protocol(Protocol::P2p(node_id));
        node_addr.add_protocol(Protocol::Memory(node_id as u64));
        let transport = Box::new(atm0s_sdn_transport_vnet::VnetTransport::new(vnet, node_id as u64, node_id, node_addr.addr()));
        let timer = Arc::new(SystemTimer());

        let router = SharedRouter::new(node_id);
        let manual = ManualBehavior::<HE, SE>::new(ManualBehaviorConf {
            node_id,
            node_addr: node_addr.addr(),
            seeds,
            local_tags: vec![],
            connect_tags: vec![],
        });
        let mut rpc_box = RpcBox::new(node_id, rpc_service_id, timer.clone());
        let rpc_behaviour = rpc_box.behaviour();
        let kv_behaviour = KeyValueBehavior::new(node_id, 3000, None);
        let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());

        let mut plane = NetworkPlane::<BE, HE, SE>::new(NetworkPlaneConfig {
            node_id,
            tick_ms: 100,
            behaviors: vec![Box::new(kv_behaviour), Box::new(router_sync_behaviour), Box::new(manual), Box::new(rpc_behaviour)],
            transport,
            timer,
            router: Arc::new(router.clone()),
        });

        let join = async_std::task::spawn(async move {
            plane.started();
            while let Ok(_) = plane.recv().await {}
            plane.stopped();
        });

        (rpc_box, node_addr.addr(), join)
    }

    #[async_std::test]
    async fn local_rpc() {
        let node_id = 1;
        let service_id = 100;
        let vnet = Arc::new(VnetEarth::default());
        let (mut rpc, _addr, join) = run_node(vnet.clone(), service_id, node_id, vec![]).await;

        let emiter = rpc.emitter();

        emiter.emit(service_id, RouteRule::ToService(0), "event1", vec![1, 2, 3]);
        assert_eq!(
            rpc.recv().timeout(Duration::from_millis(300)).await,
            Ok(Some(RpcMsg {
                cmd: "event1".to_string(),
                from_node_id: node_id,
                from_service_id: service_id,
                param: RpcMsgParam::Event(vec![1, 2, 3])
            }))
        );

        async_std::task::spawn(async move {
            let res = emiter.request(100, RouteRule::ToService(0), "echo", vec![1, 2, 3], 10000).timeout(Duration::from_secs(2)).await;
            assert_eq!(res, Ok(Ok(vec![1, 2, 3])));
        });

        let req = rpc.recv().timeout(Duration::from_millis(300)).await.unwrap().unwrap();
        assert_eq!(
            req,
            RpcMsg {
                cmd: "echo".to_string(),
                from_node_id: node_id,
                from_service_id: service_id,
                param: RpcMsgParam::Request { req_id: 0, param: vec![1, 2, 3] }
            }
        );

        let req = rpc.parse_request::<Vec<u8>, _>(req).expect("Should ok");
        req.success(vec![1, 2, 3]);

        async_std::task::sleep(Duration::from_millis(300)).await;

        join.cancel().await;
    }

    #[async_std::test]
    async fn remote_rpc() {
        let node_id1 = 1;
        let service_id1 = 100;

        let node_id2 = 2;
        let service_id2 = 200;

        let vnet = Arc::new(VnetEarth::default());

        let (mut rpc1, addr1, join1) = run_node(vnet.clone(), service_id1, node_id1, vec![]).await;
        let (mut rpc2, _addr2, join2) = run_node(vnet.clone(), service_id2, node_id2, vec![addr1]).await;

        async_std::task::sleep(Duration::from_millis(300)).await;

        let emiter1 = rpc1.emitter();

        emiter1.emit(service_id2, RouteRule::ToService(0), "event1", vec![1, 2, 3]);
        assert_eq!(
            rpc2.recv().timeout(Duration::from_millis(300)).await,
            Ok(Some(RpcMsg {
                cmd: "event1".to_string(),
                from_node_id: node_id1,
                from_service_id: service_id1,
                param: RpcMsgParam::Event(vec![1, 2, 3])
            }))
        );

        async_std::task::spawn(async move {
            let res = emiter1
                .request(service_id2, RouteRule::ToService(0), "echo", vec![1, 2, 3], 10000)
                .timeout(Duration::from_secs(2))
                .await;
            assert_eq!(res, Ok(Ok(vec![1, 2, 3])));
        });

        let req = rpc2.recv().timeout(Duration::from_millis(300)).await.unwrap().unwrap();
        assert_eq!(
            req,
            RpcMsg {
                cmd: "echo".to_string(),
                from_node_id: node_id1,
                from_service_id: service_id1,
                param: RpcMsgParam::Request { req_id: 0, param: vec![1, 2, 3] }
            }
        );

        let req = rpc2.parse_request::<Vec<u8>, _>(req).expect("Should ok");
        req.success(vec![1, 2, 3]);

        async_std::task::sleep(Duration::from_millis(300)).await;

        join1.cancel().await;
        join2.cancel().await;
    }
}

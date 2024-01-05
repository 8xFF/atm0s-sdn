#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc, time::Duration};

    use async_std::task::JoinHandle;
    use atm0s_sdn::{
        convert_enum, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent, LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent,
        ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, NetworkPlane, NetworkPlaneConfig, NodeAddr, NodeAddrBuilder, NodeId, SharedRouter, SystemTimer,
        VirtualSocketBehavior, VirtualSocketSdk, VirtualStream,
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

    async fn run_node(vnet: Arc<VnetEarth>, node_id: NodeId, seeds: Vec<NodeAddr>) -> (VirtualSocketSdk, NodeAddr, JoinHandle<()>) {
        log::info!("Run node {} connect to {:?}", node_id, seeds);
        let node_addr = Arc::new(NodeAddrBuilder::new(node_id));
        let transport = Box::new(atm0s_sdn_transport_vnet::VnetTransport::new(vnet, node_addr.addr()));
        let timer = Arc::new(SystemTimer());

        let router = SharedRouter::new(node_id);
        let manual = ManualBehavior::<HE, SE>::new(ManualBehaviorConf {
            node_id,
            node_addr: node_addr.addr(),
            seeds,
            local_tags: vec![],
            connect_tags: vec![],
        });

        let (virtual_socket_behaviour, virtual_socket_sdk) = VirtualSocketBehavior::new(node_id);
        let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());

        let mut plane = NetworkPlane::<BE, HE, SE>::new(NetworkPlaneConfig {
            node_id,
            tick_ms: 100,
            behaviors: vec![Box::new(virtual_socket_behaviour), Box::new(router_sync_behaviour), Box::new(manual)],
            transport,
            timer,
            router: Arc::new(router.clone()),
        });

        let join = async_std::task::spawn(async move {
            plane.started();
            while let Ok(_) = plane.recv().await {}
            plane.stopped();
        });

        (virtual_socket_sdk, node_addr.addr(), join)
    }

    #[async_std::test]
    async fn local_socket() {
        let node_id = 1;
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet.clone(), node_id, vec![]).await;
        async_std::task::sleep(Duration::from_millis(300)).await;

        let mut listener = sdk.listen("DEMO");
        let connector = sdk.connector();
        async_std::task::spawn(async move {
            let mut socket = connector
                .connect_to(node_id, "DEMO", HashMap::from([("k1".to_string(), "k2".to_string())]))
                .await
                .expect("Should connect");
            socket.write(&vec![1, 2, 3]).expect("Should write");
        });

        if let Some(mut socket) = listener.recv().await {
            assert_eq!(socket.meta(), &HashMap::from([("k1".to_string(), "k2".to_string())]));
            assert_eq!(socket.read().await.expect("Should read"), vec![1, 2, 3]);
            assert_eq!(socket.read().await, None);
        }

        join.cancel().await;
    }

    #[async_std::test]
    async fn local_stream() {
        let node_id = 1;
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet.clone(), node_id, vec![]).await;
        async_std::task::sleep(Duration::from_millis(300)).await;

        let mut listener = sdk.listen("DEMO");
        let connector = sdk.connector();
        async_std::task::spawn(async move {
            let socket = connector
                .connect_to(node_id, "DEMO", HashMap::from([("k1".to_string(), "k2".to_string())]))
                .await
                .expect("Should connect");
            let mut stream = VirtualStream::new(socket);
            assert_eq!(stream.write(&vec![1, 2, 3]).await.expect("Should send"), 3);
            async_std::task::sleep(Duration::from_secs(1)).await;
        });

        if let Some(socket) = listener.recv().await {
            let mut stream = VirtualStream::new(socket);
            assert_eq!(stream.meta(), &HashMap::from([("k1".to_string(), "k2".to_string())]));
            let mut buf = vec![0; 1500];
            assert_eq!(stream.read(&mut buf).await.expect("Should read"), 3);
            assert_eq!(buf[..3], [1, 2, 3]);
            assert_eq!(stream.read(&mut buf).await.expect("Should read"), 0);
        }

        join.cancel().await;
    }

    #[async_std::test]
    async fn remote_socket() {
        let vnet = Arc::new(VnetEarth::default());

        let node_id1 = 1;
        let node_id2 = 2;
        let (sdk1, addr1, join1) = run_node(vnet.clone(), node_id1, vec![]).await;
        let (sdk2, _addr2, join2) = run_node(vnet.clone(), node_id2, vec![addr1]).await;
        async_std::task::sleep(Duration::from_millis(300)).await;

        let mut listener1 = sdk1.listen("DEMO");
        let connector2 = sdk2.connector();
        async_std::task::spawn(async move {
            let mut socket = connector2
                .connect_to(node_id1, "DEMO", HashMap::from([("k1".to_string(), "k2".to_string())]))
                .await
                .expect("Should connect");
            socket.write(&vec![1, 2, 3]).expect("Should write");
        });

        if let Some(mut socket) = listener1.recv().await {
            assert_eq!(socket.meta(), &HashMap::from([("k1".to_string(), "k2".to_string())]));
            assert_eq!(socket.read().await.expect("Should read"), vec![1, 2, 3]);
            assert_eq!(socket.read().await, None);
        }

        join1.cancel().await;
        join2.cancel().await;
    }

    #[async_std::test]
    async fn remote_stream() {
        let vnet = Arc::new(VnetEarth::default());

        let node_id1 = 1;
        let node_id2 = 2;
        let (sdk1, addr1, join1) = run_node(vnet.clone(), node_id1, vec![]).await;
        let (sdk2, _addr2, join2) = run_node(vnet.clone(), node_id2, vec![addr1]).await;
        async_std::task::sleep(Duration::from_millis(300)).await;

        let mut listener1 = sdk1.listen("DEMO");
        let connector2 = sdk2.connector();
        async_std::task::spawn(async move {
            let socket = connector2
                .connect_to(node_id1, "DEMO", HashMap::from([("k1".to_string(), "k2".to_string())]))
                .await
                .expect("Should connect");
            let mut stream = VirtualStream::new(socket);
            assert_eq!(stream.write(&vec![1, 2, 3]).await.expect("Should send"), 3);
            async_std::task::sleep(Duration::from_secs(1)).await;
        });

        if let Some(socket) = listener1.recv().await {
            let mut stream = VirtualStream::new(socket);
            assert_eq!(stream.meta(), &HashMap::from([("k1".to_string(), "k2".to_string())]));
            let mut buf = vec![0; 1500];
            assert_eq!(stream.read(&mut buf).await.expect("Should read"), 3);
            assert_eq!(buf[..3], [1, 2, 3]);
            assert_eq!(stream.read(&mut buf).await.expect("Should read"), 0);
        }

        join1.cancel().await;
        join2.cancel().await;
    }
}

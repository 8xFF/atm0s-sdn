#[cfg(test)]
mod test {
    use std::{net::SocketAddrV4, sync::Arc, time::Duration};

    use async_std::task::JoinHandle;
    use atm0s_sdn::{
        convert_enum,
        virtual_socket::{create_vnet, make_insecure_quinn_client, make_insecure_quinn_server, vnet_addr, vnet_addr_v4, VirtualNet, VirtualSocketPkt},
        KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdkEvent, LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent, ManualBehavior,
        ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, NetworkPlane, NetworkPlaneConfig, NodeAddr, NodeAddrBuilder, NodeId, SharedRouter, SystemTimer,
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

    async fn run_node(vnet: Arc<VnetEarth>, node_id: NodeId, seeds: Vec<NodeAddr>) -> (VirtualNet, NodeAddr, JoinHandle<()>) {
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

        let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());
        let router = Arc::new(router);
        let (virtual_socket_behaviour, virtual_socket_sdk) = create_vnet(node_id, router.clone());

        let mut plane = NetworkPlane::<BE, HE, SE>::new(NetworkPlaneConfig {
            node_id,
            tick_ms: 100,
            behaviors: vec![Box::new(virtual_socket_behaviour), Box::new(router_sync_behaviour), Box::new(manual)],
            transport,
            timer,
            router,
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

        let server = sdk.create_udp_socket(1000, 10).expect("");
        assert_eq!(format!("{:?}", server), format!("VirtualUdpSocket {{ local_port: {} }}", server.local_port()));

        let client = sdk.create_udp_socket(0, 10).expect("");
        client.send_to(vnet_addr_v4(node_id, 1000), &vec![1, 2, 3], None).expect("Should write");
        client.send_to_node(node_id, 1000, &vec![4, 5, 6], None).expect("Should write");
        assert_eq!(
            server.try_recv_from(),
            Some(VirtualSocketPkt {
                src: SocketAddrV4::new(node_id.into(), client.local_port()),
                payload: vec![1, 2, 3],
                ecn: None,
            })
        );
        assert_eq!(
            server.try_recv_from(),
            Some(VirtualSocketPkt {
                src: SocketAddrV4::new(node_id.into(), client.local_port()),
                payload: vec![4, 5, 6],
                ecn: None,
            })
        );
        assert_eq!(server.try_recv_from(), None);

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

        let server1 = sdk1.create_udp_socket(1000, 10).expect("");
        let client2 = sdk2.create_udp_socket(0, 10).expect("");
        client2.send_to(vnet_addr_v4(node_id1, 1000), &vec![1, 2, 3], None).expect("Should write");

        assert_eq!(
            server1.recv_from().await,
            Some(VirtualSocketPkt {
                src: SocketAddrV4::new(node_id2.into(), client2.local_port()),
                payload: vec![1, 2, 3],
                ecn: None,
            })
        );

        join1.cancel().await;
        join2.cancel().await;
    }

    #[async_std::test]
    async fn local_quinn() {
        let node_id = 1;
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet.clone(), node_id, vec![]).await;
        async_std::task::sleep(Duration::from_millis(300)).await;

        let server = make_insecure_quinn_server(sdk.create_udp_socket(1000, 10).expect("")).expect("");
        let client = make_insecure_quinn_client(sdk.create_udp_socket(0, 10).expect("")).expect("");

        async_std::task::spawn(async move {
            let connection = client.connect(vnet_addr(node_id, 1000), "localhost").unwrap().await.unwrap();
            let (mut send, mut recv) = connection.open_bi().await.unwrap();
            send.write(&vec![4, 5, 6]).await.unwrap();
            let mut buf = vec![0; 3];
            let len = recv.read(&mut buf).await.unwrap();
            assert_eq!(buf, vec![1, 2, 3]);
            assert_eq!(len, Some(3));
            send.write(&vec![7, 8, 9]).await.unwrap();
            send.finish().await.unwrap();
        });

        let connection = server.accept().await.unwrap().await.unwrap();
        let (mut send, mut recv) = connection.accept_bi().await.unwrap();
        let mut buf = vec![0; 3];
        let len = recv.read(&mut buf).await.unwrap();
        assert_eq!(buf, vec![4, 5, 6]);
        assert_eq!(len, Some(3));
        send.write(&vec![1, 2, 3]).await.unwrap();
        let len = recv.read(&mut buf).await.unwrap();
        assert_eq!(buf, vec![7, 8, 9]);
        assert_eq!(len, Some(3));
        join.cancel().await;
    }

    #[async_std::test]
    async fn remote_quinn() {
        let vnet = Arc::new(VnetEarth::default());

        let node_id1 = 1;
        let node_id2 = 2;
        let (sdk1, addr1, join1) = run_node(vnet.clone(), node_id1, vec![]).await;
        let (sdk2, _addr2, join2) = run_node(vnet.clone(), node_id2, vec![addr1]).await;
        async_std::task::sleep(Duration::from_millis(300)).await;

        let server1 = make_insecure_quinn_server(sdk1.create_udp_socket(1000, 10).expect("")).expect("");
        let client2 = make_insecure_quinn_client(sdk2.create_udp_socket(0, 10).expect("")).expect("");

        async_std::task::spawn(async move {
            let connection = client2.connect(vnet_addr(node_id1, 1000), "localhost").unwrap().await.unwrap();
            let (mut send, mut recv) = connection.open_bi().await.unwrap();
            send.write(&vec![4, 5, 6]).await.unwrap();
            let mut buf = vec![0; 3];
            let len = recv.read(&mut buf).await.unwrap();
            assert_eq!(buf, vec![1, 2, 3]);
            assert_eq!(len, Some(3));
            send.write(&vec![7, 8, 9]).await.unwrap();
            send.finish().await.unwrap();
        });

        let connection = server1.accept().await.unwrap().await.unwrap();
        let (mut send, mut recv) = connection.accept_bi().await.unwrap();
        let mut buf = vec![0; 3];
        let len = recv.read(&mut buf).await.unwrap();
        assert_eq!(buf, vec![4, 5, 6]);
        assert_eq!(len, Some(3));
        send.write(&vec![1, 2, 3]).await.unwrap();
        let len = recv.read(&mut buf).await.unwrap();
        assert_eq!(buf, vec![7, 8, 9]);
        assert_eq!(len, Some(3));

        join1.cancel().await;
        join2.cancel().await;
    }
}

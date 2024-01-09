#[cfg(test)]
mod tests {
    use async_std::prelude::FutureExt;
    use async_std::task::JoinHandle;
    use atm0s_sdn::{convert_enum, NetworkPlane, NetworkPlaneConfig};
    use atm0s_sdn::{KeyValueBehavior, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdk, KeyValueSdkEvent};
    use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
    use atm0s_sdn::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
    use atm0s_sdn::{NodeAddr, NodeAddrBuilder, NodeId, PubsubServiceBehaviour, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent};
    use atm0s_sdn::{NodeAliasBehavior, NodeAliasId, NodeAliasResult, NodeAliasSdk, SharedRouter};
    use atm0s_sdn::{OptionUtils, SystemTimer};
    use atm0s_sdn_transport_vnet::VnetEarth;
    use std::{sync::Arc, time::Duration, vec};

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplBehaviorEvent {
        Pubsub(PubsubServiceBehaviourEvent),
        KeyValue(KeyValueBehaviorEvent),
        RouterSync(LayersSpreadRouterSyncBehaviorEvent),
        Manual(ManualBehaviorEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplHandlerEvent {
        Pubsub(PubsubServiceHandlerEvent),
        KeyValue(KeyValueHandlerEvent),
        RouterSync(LayersSpreadRouterSyncHandlerEvent),
        Manual(ManualHandlerEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplSdkEvent {
        KeyValue(KeyValueSdkEvent),
    }

    async fn run_node(vnet: Arc<VnetEarth>, node_id: NodeId, seeds: Vec<NodeAddr>) -> (NodeAliasSdk, NodeAddr, JoinHandle<()>) {
        log::info!("Run node {} connect to {:?}", node_id, seeds);
        let node_addr = Arc::new(NodeAddrBuilder::new(node_id));
        let transport = Box::new(atm0s_sdn_transport_vnet::VnetTransport::new(vnet, node_addr.addr()));
        let timer = Arc::new(SystemTimer());

        let router = SharedRouter::new(node_id);
        let manual = ManualBehavior::new(ManualBehaviorConf {
            node_id,
            node_addr: node_addr.addr(),
            seeds,
            local_tags: vec![],
            connect_tags: vec![],
        });

        let router_sync_behaviour = LayersSpreadRouterSyncBehavior::new(router.clone());
        let kv_sdk = KeyValueSdk::new();
        let kv_behaviour = KeyValueBehavior::new(node_id, 1000, Some(Box::new(kv_sdk.clone())));
        let (pubsub_behavior, pubsub_sdk) = PubsubServiceBehaviour::new(node_id, timer.clone());
        let (node_alias_behavior, node_alias_sdk) = NodeAliasBehavior::new(node_id, pubsub_sdk);

        let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent, ImplSdkEvent>::new(NetworkPlaneConfig {
            node_id,
            tick_ms: 100,
            behaviors: vec![
                Box::new(pubsub_behavior),
                Box::new(kv_behaviour),
                Box::new(router_sync_behaviour),
                Box::new(manual),
                Box::new(node_alias_behavior),
            ],
            transport,
            timer,
            router: Arc::new(router.clone()),
        });

        let join = async_std::task::spawn(async move {
            plane.started();
            while let Ok(_) = plane.recv().await {}
            plane.stopped();
        });

        (node_alias_sdk, node_addr.addr(), join)
    }

    /// Testing local alias
    #[async_std::test]
    async fn local_node() {
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet, 1, vec![]).await;

        async_std::task::sleep(Duration::from_millis(300)).await;
        let node_alias: NodeAliasId = 1000.into();

        sdk.register(node_alias.clone());

        let (tx, rx) = async_std::channel::bounded(1);
        sdk.find_alias(
            node_alias.clone(),
            Box::new(move |res| {
                tx.try_send(res).expect("");
            }),
        );
        assert_eq!(rx.recv().timeout(Duration::from_millis(100)).await.unwrap().unwrap(), Ok(NodeAliasResult::FromLocal));

        sdk.unregister(node_alias.clone());
        let (tx, rx) = async_std::channel::bounded(1);
        sdk.find_alias(
            node_alias.clone(),
            Box::new(move |res| {
                tx.try_send(res).expect("");
            }),
        );
        assert!(rx.recv().timeout(Duration::from_millis(100)).await.is_err());

        join.cancel().await.print_none("Should cancel join");
    }

    /// Testing remote alias with scan
    #[async_std::test]
    async fn remote_node_scan() {
        let vnet = Arc::new(VnetEarth::default());
        let (sdk1, addr1, join1) = run_node(vnet.clone(), 1, vec![]).await;
        async_std::task::sleep(Duration::from_millis(300)).await;

        let node_alias: NodeAliasId = 1000.into();
        sdk1.register(node_alias.clone());

        let (sdk2, _addr2, join2) = run_node(vnet, 2, vec![addr1]).await;
        //Need to wait pub-sub ready
        async_std::task::sleep(Duration::from_millis(2000)).await;

        let (tx, rx) = async_std::channel::bounded(1);
        sdk2.find_alias(
            node_alias.clone(),
            Box::new(move |res| {
                tx.try_send(res).expect("");
            }),
        );
        assert_eq!(rx.recv().timeout(Duration::from_millis(100)).await.unwrap().unwrap(), Ok(NodeAliasResult::FromScan(1)));

        sdk1.unregister(node_alias.clone());
        let (tx, rx) = async_std::channel::bounded(1);
        sdk2.find_alias(
            node_alias.clone(),
            Box::new(move |res| {
                tx.try_send(res).expect("");
            }),
        );
        assert!(rx.recv().timeout(Duration::from_millis(100)).await.is_err());

        join1.cancel().await.print_none("Should cancel join");
        join2.cancel().await.print_none("Should cancel join");
    }

    /// Testing remote alias
    #[async_std::test]
    async fn remote_node_find() {
        let vnet = Arc::new(VnetEarth::default());
        let (sdk1, addr1, join1) = run_node(vnet.clone(), 1, vec![]).await;
        let (sdk2, _addr2, join2) = run_node(vnet, 2, vec![addr1]).await;

        async_std::task::sleep(Duration::from_millis(300)).await;
        let node_alias: NodeAliasId = 1000.into();

        //Need to wait pub-sub ready
        async_std::task::sleep(Duration::from_millis(2000)).await;

        //we will register after pubsub ready => it will be registered on remote node
        sdk1.register(node_alias.clone());

        //wait for remote register done
        async_std::task::sleep(Duration::from_millis(300)).await;

        let (tx, rx) = async_std::channel::bounded(1);
        sdk2.find_alias(
            node_alias.clone(),
            Box::new(move |res| {
                tx.try_send(res).expect("");
            }),
        );
        assert_eq!(rx.recv().timeout(Duration::from_millis(100)).await.unwrap().unwrap(), Ok(NodeAliasResult::FromHint(1)));

        sdk1.unregister(node_alias.clone());
        let (tx, rx) = async_std::channel::bounded(1);
        sdk2.find_alias(
            node_alias.clone(),
            Box::new(move |res| {
                tx.try_send(res).expect("");
            }),
        );
        assert!(rx.recv().timeout(Duration::from_millis(100)).await.is_err());

        join1.cancel().await.print_none("Should cancel join");
        join2.cancel().await.print_none("Should cancel join");
    }
}

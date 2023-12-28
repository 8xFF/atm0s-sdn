#[cfg(test)]
mod tests {
    use async_std::prelude::FutureExt;
    use async_std::task::JoinHandle;
    use atm0s_sdn::SharedRouter;
    use atm0s_sdn::{convert_enum, NetworkPlane, NetworkPlaneConfig};
    use atm0s_sdn::{KeyValueBehavior, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueSdk, KeyValueSdkEvent};
    use atm0s_sdn::{LayersSpreadRouterSyncBehavior, LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
    use atm0s_sdn::{ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent};
    use atm0s_sdn::{NodeAddr, NodeAddrBuilder, NodeId};
    use atm0s_sdn::{OptionUtils, SystemTimer};
    use atm0s_sdn_transport_vnet::VnetEarth;
    use std::{sync::Arc, time::Duration, vec};

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplBehaviorEvent {
        KeyValue(KeyValueBehaviorEvent),
        RouterSync(LayersSpreadRouterSyncBehaviorEvent),
        Manual(ManualBehaviorEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplHandlerEvent {
        KeyValue(KeyValueHandlerEvent),
        RouterSync(LayersSpreadRouterSyncHandlerEvent),
        Manual(ManualHandlerEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto)]
    enum ImplSdkEvent {
        KeyValue(KeyValueSdkEvent),
    }

    async fn run_node(vnet: Arc<VnetEarth>, node_id: NodeId, seeds: Vec<NodeAddr>) -> (KeyValueSdk, NodeAddr, JoinHandle<()>) {
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
        let kv_behaviour = KeyValueBehavior::new(node_id, 3000, Some(Box::new(kv_sdk.clone())));

        let mut plane = NetworkPlane::<ImplBehaviorEvent, ImplHandlerEvent, ImplSdkEvent>::new(NetworkPlaneConfig {
            node_id,
            tick_ms: 100,
            behaviors: vec![Box::new(kv_behaviour), Box::new(router_sync_behaviour), Box::new(manual)],
            transport,
            timer,
            router: Arc::new(router.clone()),
        });

        let join = async_std::task::spawn(async move {
            plane.started();
            while let Ok(_) = plane.recv().await {}
            plane.stopped();
        });

        (kv_sdk, node_addr.addr(), join)
    }

    #[async_std::test]
    async fn local_node_simple() {
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet, 1, vec![]).await;

        async_std::task::sleep(Duration::from_millis(300)).await;

        //for simple key-value
        const KEY_ID: u64 = 1000;
        let mut event_rx = sdk.subscribe(KEY_ID, None);
        sdk.set(KEY_ID, vec![1, 2, 3], None);

        let (key, value, _, source) = event_rx.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(value, Some(vec![1, 2, 3]));
        assert_eq!(source, 1);

        sdk.del(KEY_ID);
        let (key, value, _, source) = event_rx.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(value, None);
        assert_eq!(source, 1);

        join.cancel().await.print_none("Should cancel join");
    }

    #[async_std::test]
    async fn local_node_hmap() {
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet, 1, vec![]).await;

        async_std::task::sleep(Duration::from_millis(300)).await;

        //for simple key-value
        const KEY_ID: u64 = 1000;
        const SUB_KEY: u64 = 2000;
        let mut event_rx = sdk.hsubscribe(KEY_ID, None);
        sdk.hset(KEY_ID, SUB_KEY, vec![1, 2, 3], None);

        let (key, sub_key, value, _, source) = event_rx.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, Some(vec![1, 2, 3]));
        assert_eq!(source, 1);

        sdk.hdel(KEY_ID, SUB_KEY);

        let (key, sub_key, value, _, source) = event_rx.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, None);
        assert_eq!(source, 1);

        join.cancel().await.print_none("Should cancel join");
    }

    #[async_std::test]
    async fn local_node_hmap_raw() {
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet, 1, vec![]).await;

        async_std::task::sleep(Duration::from_millis(300)).await;

        //for simple key-value
        const KEY_ID: u64 = 1000;
        const SUB_KEY: u64 = 2000;
        let (tx2, rx2) = async_std::channel::bounded(10);
        sdk.hsubscribe_raw(KEY_ID, 2000, None, tx2);

        sdk.hset(KEY_ID, SUB_KEY, vec![1, 2, 3], None);

        let (key, sub_key, value, _, source) = rx2.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, Some(vec![1, 2, 3]));
        assert_eq!(source, 1);

        sdk.hdel(KEY_ID, SUB_KEY);

        let (key, sub_key, value, _, source) = rx2.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, None);
        assert_eq!(source, 1);

        join.cancel().await.print_none("Should cancel join");
    }

    #[async_std::test]
    async fn local_node_hmap_both() {
        let vnet = Arc::new(VnetEarth::default());
        let (sdk, _addr, join) = run_node(vnet, 1, vec![]).await;

        async_std::task::sleep(Duration::from_millis(300)).await;

        //for simple key-value
        const KEY_ID: u64 = 1000;
        const SUB_KEY: u64 = 2000;
        let (tx2, rx2) = async_std::channel::bounded(10);
        let mut event_rx = sdk.hsubscribe(KEY_ID, None);
        sdk.hsubscribe_raw(KEY_ID, 20000, None, tx2);

        sdk.hset(KEY_ID, SUB_KEY, vec![1, 2, 3], None);

        let (key, sub_key, value, _, source) = event_rx.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, Some(vec![1, 2, 3]));
        assert_eq!(source, 1);

        let (key, sub_key, value, _, source) = rx2.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, Some(vec![1, 2, 3]));
        assert_eq!(source, 1);

        sdk.hdel(KEY_ID, SUB_KEY);

        let (key, sub_key, value, _, source) = event_rx.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, None);
        assert_eq!(source, 1);

        let (key, sub_key, value, _, source) = rx2.recv().timeout(Duration::from_millis(300)).await.expect("Should receive event").expect("Should has event");
        assert_eq!(key, KEY_ID);
        assert_eq!(sub_key, SUB_KEY);
        assert_eq!(value, None);
        assert_eq!(source, 1);

        join.cancel().await.print_none("Should cancel join");
    }
}

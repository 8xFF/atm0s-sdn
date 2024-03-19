use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
};

use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
use atm0s_sdn_utils::hash::hash_str;

use crate::{
    base::{ConnectionEvent, Service, ServiceBuilder, ServiceInput, ServiceOutput, ServiceSharedInput, ServiceWorker},
    features::{
        dht_kv::{Control as KvControl, Event as KvEvent, Key, Map, MapControl, MapEvent},
        neighbours::Control as NeighbourControl,
        FeaturesControl, FeaturesEvent,
    },
};

const RETRY_CONNECT_MS: u64 = 60_000; //10 seconds

pub const SERVICE_ID: u8 = 0;
pub const SERVICE_NAME: &str = "manual_discovery";

fn kv_control<TW>(c: KvControl) -> ServiceOutput<FeaturesControl, TW> {
    ServiceOutput::FeatureControl(FeaturesControl::DhtKv(c))
}

fn neighbour_control<TW>(c: NeighbourControl) -> ServiceOutput<FeaturesControl, TW> {
    ServiceOutput::FeatureControl(FeaturesControl::Neighbours(c))
}

pub struct ManualDiscoveryService<TC, TW> {
    queue: VecDeque<ServiceOutput<FeaturesControl, TW>>,
    nodes: HashMap<NodeId, NodeAddr>,
    conns: HashMap<NodeId, Vec<ConnId>>,
    removing_list: Vec<NodeId>,
    last_retry_ms: u64,
    _tmp: std::marker::PhantomData<(TC, TW)>,
}

impl<TC, TW> ManualDiscoveryService<TC, TW> {
    pub fn new(node_addr: NodeAddr, local_tags: Vec<String>, connect_tags: Vec<String>) -> Self {
        log::info!("Creating ManualDiscoveryService for node {node_addr} with local tags {local_tags:?} and connect tags {connect_tags:?}");

        let mut queue = VecDeque::new();

        for local_tag in local_tags.iter() {
            let map = Map(hash_str(local_tag));
            log::info!("Setting local tag: {local_tag} by set key {map}");
            queue.push_back(kv_control(KvControl::MapCmd(map, MapControl::Set(Key(0), node_addr.to_vec()))));
        }

        for connect_tag in connect_tags.iter() {
            let map = Map(hash_str(connect_tag));
            log::info!("Setting connect tag: {connect_tag} by sub key {map}");
            queue.push_back(kv_control(KvControl::MapCmd(map, MapControl::Sub)));
        }

        Self {
            nodes: HashMap::new(),
            conns: HashMap::new(),
            queue,
            removing_list: vec![],
            last_retry_ms: 0,
            _tmp: std::marker::PhantomData,
        }
    }

    fn check_nodes(&mut self, now: u64) {
        if self.last_retry_ms + RETRY_CONNECT_MS <= now {
            self.last_retry_ms = now;
            for (node, addr) in self.nodes.iter() {
                if !self.conns.contains_key(node) {
                    log::info!("ManualDiscoveryService node {node} not connected, retry connect");
                    self.queue.push_back(neighbour_control(NeighbourControl::ConnectTo(addr.clone())));
                }
            }
        }

        while let Some(node) = self.removing_list.pop() {
            if !self.nodes.contains_key(&node) {
                log::info!("ManualDiscoveryService node {node} still in removing_list => send Disconnect");
                self.queue.push_back(neighbour_control(NeighbourControl::DisconnectFrom(node)));
            }
        }
    }
}

impl<TC: Debug, TW: Debug> Service<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryService<TC, TW> {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn on_shared_input<'a>(&mut self, now: u64, input: ServiceSharedInput) {
        match input {
            ServiceSharedInput::Tick(_) => self.check_nodes(now),
            ServiceSharedInput::Connection(ConnectionEvent::Connected(ctx, _)) => {
                let entry = self.conns.entry(ctx.node).or_insert_with(Vec::new);
                entry.push(ctx.conn);
            }
            ServiceSharedInput::Connection(ConnectionEvent::Disconnected(ctx)) => {
                let entry = self.conns.entry(ctx.node).or_insert_with(Vec::new);
                entry.retain(|&conn| conn != ctx.conn);

                if entry.is_empty() {
                    log::info!("ManualDiscoveryService node {} disconnected all connections => remove", ctx.node);
                    self.conns.remove(&ctx.node);
                }
            }
            _ => {}
        }
    }

    fn on_input(&mut self, _now: u64, input: ServiceInput<FeaturesEvent, TC>) {
        match input {
            ServiceInput::FeatureEvent(FeaturesEvent::DhtKv(KvEvent::MapEvent(map, event))) => match event {
                MapEvent::OnSet(_, source, value) => {
                    if let Some(addr) = NodeAddr::from_vec(&value) {
                        log::info!("ManualDiscoveryService node {source} added tag {map} => connect {addr}");
                        self.nodes.insert(source, addr.clone());
                        self.queue.push_back(neighbour_control(NeighbourControl::ConnectTo(addr)));
                        self.removing_list.retain(|&node| node != source);
                    }
                }
                MapEvent::OnDel(_, source) => {
                    self.nodes.remove(&source);
                    if !self.removing_list.contains(&source) {
                        log::info!("ManualDiscoveryService node {source} removed tag {map} => push to removing_list");
                        self.removing_list.push(source);
                    }
                }
                MapEvent::OnRelaySelected(node) => {
                    log::info!("ManualDiscoveryService relay {node} selected for tag {map}");
                }
            },
            _ => {}
        }
    }

    fn pop_output(&mut self) -> Option<ServiceOutput<FeaturesControl, TW>> {
        self.queue.pop_front()
    }
}

pub struct ManualDiscoveryServiceWorker {}

impl<TC, TW> ServiceWorker<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryServiceWorker {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }
}

pub struct ManualDiscoveryServiceBuilder<TC, TW> {
    _tmp: std::marker::PhantomData<(TC, TW)>,
    node_addr: NodeAddr,
    local_tags: Vec<String>,
    connect_tags: Vec<String>,
}

impl<TC, TW> ManualDiscoveryServiceBuilder<TC, TW> {
    pub fn new(node_addr: NodeAddr, local_tags: Vec<String>, connect_tags: Vec<String>) -> Self {
        Self {
            _tmp: std::marker::PhantomData,
            node_addr,
            local_tags,
            connect_tags,
        }
    }
}

impl<TC: 'static + Debug + Send + Sync, TW: 'static + Debug + Send + Sync> ServiceBuilder<FeaturesControl, FeaturesEvent, TC, TW> for ManualDiscoveryServiceBuilder<TC, TW> {
    fn service_id(&self) -> u8 {
        SERVICE_ID
    }

    fn service_name(&self) -> &str {
        SERVICE_NAME
    }

    fn create(&self) -> Box<dyn Service<FeaturesControl, FeaturesEvent, TC, TW>> {
        Box::new(ManualDiscoveryService::new(self.node_addr.clone(), self.local_tags.clone(), self.connect_tags.clone()))
    }

    fn create_worker(&self) -> Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, TC, TW>> {
        Box::new(ManualDiscoveryServiceWorker {})
    }
}

#[cfg(test)]
mod test {
    use atm0s_sdn_identity::{NodeAddrBuilder, Protocol};
    use atm0s_sdn_utils::hash::hash_str;

    use crate::{
        base::{Service, ServiceInput, ServiceOutput},
        features::{
            dht_kv::{self, Key, Map, MapControl, MapEvent},
            neighbours, FeaturesControl,
        },
    };

    use super::ManualDiscoveryService;

    #[test]
    fn should_send_connect() {
        let addr1 = {
            let mut builder = NodeAddrBuilder::new(1);
            builder.add_protocol(Protocol::Ip4([127, 0, 0, 1].into()));
            builder.add_protocol(Protocol::Udp(100));
            builder.addr()
        };

        let addr2 = {
            let mut builder = NodeAddrBuilder::new(1);
            builder.add_protocol(Protocol::Ip4([127, 0, 0, 1].into()));
            builder.add_protocol(Protocol::Udp(101));
            builder.addr()
        };

        let mut service = ManualDiscoveryService::<(), ()>::new(addr1.clone(), vec!["local".into()], vec!["connect".into()]);
        let local_map = Map(hash_str("local"));
        let connect_map = Map(hash_str("connect"));

        assert_eq!(
            service.pop_output(),
            Some(ServiceOutput::FeatureControl(FeaturesControl::DhtKv(dht_kv::Control::MapCmd(
                local_map,
                MapControl::Set(Key(0), addr1.to_vec())
            ))))
        );
        assert_eq!(
            service.pop_output(),
            Some(ServiceOutput::FeatureControl(FeaturesControl::DhtKv(dht_kv::Control::MapCmd(connect_map, MapControl::Sub))))
        );

        service.on_input(
            100,
            ServiceInput::FeatureEvent(crate::features::FeaturesEvent::DhtKv(dht_kv::Event::MapEvent(connect_map, MapEvent::OnSet(Key(1), 2, addr2.to_vec())))),
        );
        assert_eq!(
            service.pop_output(),
            Some(ServiceOutput::FeatureControl(FeaturesControl::Neighbours(neighbours::Control::ConnectTo(addr2))))
        );
    }

    //TODO test more case
}

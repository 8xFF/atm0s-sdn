use std::{collections::VecDeque, fmt::Debug};

use atm0s_sdn_identity::NodeAddr;
use atm0s_sdn_utils::hash::hash_str;

use crate::{
    base::{Service, ServiceBuilder, ServiceInput, ServiceOutput, ServiceSharedInput, ServiceWorker},
    features::{
        dht_kv::{Control as KvControl, Event as KvEvent, Key, Map, MapControl, MapEvent},
        neighbours::Control as NeighbourControl,
        FeaturesControl, FeaturesEvent,
    },
};

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
            queue,
            _tmp: std::marker::PhantomData,
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

    fn on_shared_input<'a>(&mut self, _now: u64, input: ServiceSharedInput) {}

    fn on_input(&mut self, _now: u64, input: ServiceInput<FeaturesEvent, TC>) {
        match input {
            ServiceInput::FeatureEvent(FeaturesEvent::DhtKv(KvEvent::MapEvent(map, event))) => match event {
                MapEvent::OnSet(_, source, value) => {
                    if let Some(addr) = NodeAddr::from_vec(&value) {
                        log::info!("ManualDiscoveryService node {source} added tag {map} => connect {addr}");
                        self.queue.push_back(neighbour_control(NeighbourControl::ConnectTo(addr)));
                    }
                }
                MapEvent::OnDel(_, source) => {
                    log::info!("ManualDiscoveryService node {source} removed tag {map} => disconnect");
                    self.queue.push_back(neighbour_control(NeighbourControl::DisconnectFrom(source)));
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

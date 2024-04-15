use std::sync::Arc;

use atm0s_sdn_network::{
    base::{Service, ServiceBuilder, ServiceCtx, ServiceInput, ServiceOutput, ServiceSharedInput, ServiceWorker},
    features::{
        alias::{self, FoundLocation},
        FeaturesControl, FeaturesEvent,
    },
    ExtIn, ExtOut,
};
use atm0s_sdn_router::ServiceBroadcastLevel;

use crate::simulator::{NetworkSimulator, TestNode};

mod simulator;

struct MockService;

impl Service<FeaturesControl, FeaturesEvent, (), (), (), ()> for MockService {
    fn service_id(&self) -> u8 {
        0
    }

    fn service_name(&self) -> &str {
        "mock"
    }

    fn on_input(&mut self, _ctx: &ServiceCtx, _now: u64, _input: ServiceInput<FeaturesEvent, (), ()>) {}

    fn on_shared_input<'a>(&mut self, _ctx: &ServiceCtx, _now: u64, _input: ServiceSharedInput) {}

    fn pop_output(&mut self, _ctx: &ServiceCtx) -> Option<ServiceOutput<FeaturesControl, (), ()>> {
        None
    }
}

struct MockServiceWorker;

impl ServiceWorker<FeaturesControl, FeaturesEvent, (), (), (), ()> for MockServiceWorker {
    fn service_id(&self) -> u8 {
        0
    }

    fn service_name(&self) -> &str {
        "mock"
    }
}

struct MockServiceBuilder;

impl ServiceBuilder<FeaturesControl, FeaturesEvent, (), (), (), ()> for MockServiceBuilder {
    fn service_id(&self) -> u8 {
        0
    }

    fn service_name(&self) -> &str {
        "mock"
    }

    fn create(&self) -> Box<dyn Service<FeaturesControl, FeaturesEvent, (), (), (), ()>> {
        Box::new(MockService)
    }

    fn create_worker(&self) -> Box<dyn ServiceWorker<FeaturesControl, FeaturesEvent, (), (), (), ()>> {
        Box::new(MockServiceWorker)
    }
}

#[test]
fn feature_alias_single_node() {
    let node1 = 1;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![Arc::new(MockServiceBuilder)]));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    let alias = 1000;
    let service = 0;
    let level = ServiceBroadcastLevel::Global;

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Alias(alias::Control::Register { alias, service, level })));
    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Alias(alias::Control::Query { alias, service, level })));
    sim.process(10);
    assert_eq!(
        sim.pop_res(),
        Some((node1, ExtOut::FeaturesEvent(FeaturesEvent::Alias(alias::Event::QueryResult(alias, Some(FoundLocation::Local))))))
    );
}

#[test]
fn feature_alias_timeout() {
    let node1 = 1;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![Arc::new(MockServiceBuilder)]));

    sim.process(10);

    let alias_v = 1000;
    let service = 0;
    let level = ServiceBroadcastLevel::Global;

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Alias(alias::Control::Query { alias: alias_v, service, level })));
    sim.process(10);
    sim.process(alias::HINT_TIMEOUT_MS);
    sim.process(alias::SCAN_TIMEOUT_MS);
    assert_eq!(sim.pop_res(), Some((node1, ExtOut::FeaturesEvent(FeaturesEvent::Alias(alias::Event::QueryResult(alias_v, None))))));
}

#[test]
fn feature_alias_two_nodes() {
    let node1 = 1;
    let node2 = 2;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![Arc::new(MockServiceBuilder)]));
    let addr2 = sim.add_node(TestNode::new(node2, 1235, vec![Arc::new(MockServiceBuilder)]));

    sim.control(node1, ExtIn::ConnectTo(addr2));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    let alias_v = 1000;
    let service = 0;
    let level = ServiceBroadcastLevel::Global;

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Alias(alias::Control::Register { alias: alias_v, service, level })));
    sim.process(10);
    sim.process(alias::HINT_TIMEOUT_MS);
    sim.control(node2, ExtIn::FeaturesControl(FeaturesControl::Alias(alias::Control::Query { alias: alias_v, service, level })));
    sim.process(10);
    assert_eq!(
        sim.pop_res(),
        Some((
            node2,
            ExtOut::FeaturesEvent(FeaturesEvent::Alias(alias::Event::QueryResult(alias_v, Some(FoundLocation::RemoteHint(node1)))))
        ))
    );
}

#[test]
fn feature_alias_three_nodes() {
    // node1 <-> node2 <-> node3
    let node1 = 1;
    let node2 = 2;
    let node3 = 3;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![Arc::new(MockServiceBuilder)]));
    let addr2 = sim.add_node(TestNode::new(node2, 1235, vec![Arc::new(MockServiceBuilder)]));

    sim.control(node1, ExtIn::ConnectTo(addr2));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    let alias = 1000;
    let service = 0;
    let level = ServiceBroadcastLevel::Global;

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Alias(alias::Control::Register { alias, service, level })));
    sim.process(10);

    let addr3 = sim.add_node(TestNode::new(node3, 1236, vec![Arc::new(MockServiceBuilder)]));
    sim.control(node2, ExtIn::ConnectTo(addr3));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    sim.control(node3, ExtIn::FeaturesControl(FeaturesControl::Alias(alias::Control::Query { alias, service, level })));
    sim.process(10);
    assert_eq!(
        sim.pop_res(),
        Some((
            node3,
            ExtOut::FeaturesEvent(FeaturesEvent::Alias(alias::Event::QueryResult(alias, Some(FoundLocation::RemoteScan(node1)))))
        ))
    );
}

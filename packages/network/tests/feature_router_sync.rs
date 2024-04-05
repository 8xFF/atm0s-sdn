use std::sync::Arc;

use atm0s_sdn_network::{
    base::{NetIncomingMeta, NetOutgoingMeta, Service, ServiceBuilder, ServiceCtx, ServiceInput, ServiceOutput, ServiceSharedInput, ServiceWorker},
    features::{data, FeaturesControl, FeaturesEvent},
    ExtIn, ExtOut,
};
use atm0s_sdn_router::RouteRule;

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
fn feature_router_sync_single_node() {
    let node1 = 1;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![Arc::new(MockServiceBuilder)]));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Data(data::Control::Ping(node1))));
    sim.process(10);
    assert_eq!(sim.pop_res(), Some((node1, ExtOut::FeaturesEvent(FeaturesEvent::Data(data::Event::Pong(node1, Some(0)))))));

    sim.control(
        node1,
        ExtIn::FeaturesControl(FeaturesControl::Data(data::Control::SendRule(RouteRule::ToService(0), NetOutgoingMeta::default(), vec![1, 2, 3, 4]))),
    );
    sim.process(10);
    assert_eq!(
        sim.pop_res(),
        Some((node1, ExtOut::FeaturesEvent(FeaturesEvent::Data(data::Event::Recv(NetIncomingMeta::default(), vec![1, 2, 3, 4])))))
    );
}

#[test]
fn feature_router_sync_two_nodes() {
    let node1 = 1;
    let node2 = 2;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![]));
    let addr2 = sim.add_node(TestNode::new(node2, 1235, vec![]));

    sim.control(node1, ExtIn::ConnectTo(addr2));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Data(data::Control::Ping(node2))));
    sim.process(10);
    assert_eq!(sim.pop_res(), Some((node1, ExtOut::FeaturesEvent(FeaturesEvent::Data(data::Event::Pong(node2, Some(0)))))));
}

#[test]
fn feature_router_sync_three_nodes() {
    // node1 <-> node2 <-> node3
    let node1 = 1;
    let node2 = 2;
    let node3 = 3;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![]));
    let addr2 = sim.add_node(TestNode::new(node2, 1235, vec![]));
    let addr3 = sim.add_node(TestNode::new(node3, 1236, vec![]));

    sim.control(node1, ExtIn::ConnectTo(addr2));
    sim.control(node2, ExtIn::ConnectTo(addr3));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Data(data::Control::Ping(node3))));
    sim.process(10);
    assert_eq!(sim.pop_res(), Some((node1, ExtOut::FeaturesEvent(FeaturesEvent::Data(data::Event::Pong(node3, Some(0)))))));
}

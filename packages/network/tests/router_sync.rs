use atm0s_sdn_network::{
    features::{data, FeaturesControl, FeaturesEvent},
    ExtIn, ExtOut,
};

use crate::simulator::{NetworkSimulator, TestNode};

mod simulator;

#[test]
fn router_sync_two_nodes() {
    let node1 = 1;
    let node2 = 2;
    let mut sim = NetworkSimulator::<(), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234));
    let addr2 = sim.add_node(TestNode::new(node2, 1235));

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
fn router_sync_three_nodes() {
    // node1 <-> node2 <-> node3
    let node1 = 1;
    let node2 = 2;
    let node3 = 2;
    let mut sim = NetworkSimulator::<(), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234));
    let addr2 = sim.add_node(TestNode::new(node2, 1235));
    let addr3 = sim.add_node(TestNode::new(node2, 1236));

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

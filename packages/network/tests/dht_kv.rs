use atm0s_sdn_network::{
    features::{
        dht_kv::{Control, Event, Key, Map, MapControl, MapEvent},
        FeaturesControl, FeaturesEvent,
    },
    ExtIn, ExtOut,
};

use crate::simulator::{NetworkSimulator, TestNode};

mod simulator;

fn control(control: Control) -> ExtIn {
    ExtIn::FeaturesControl(FeaturesControl::DhtKv(control))
}

fn event(event: Event) -> ExtOut {
    ExtOut::FeaturesEvent(FeaturesEvent::DhtKv(event))
}

#[test]
fn single_node() {
    let node_id = 1;
    let mut sim = NetworkSimulator::<(), ()>::new(0);
    sim.add_node(TestNode::new(node_id, 1234));

    sim.process(100);

    let key = Map(1000);
    let sub_key = Key(2000);
    let value = vec![1, 2, 3, 4];

    sim.control(node_id, control(Control::MapCmd(key, MapControl::Sub)));
    sim.process(100);

    sim.control(node_id, control(Control::MapCmd(key, MapControl::Set(sub_key, value.clone()))));
    sim.process(100);

    assert_eq!(sim.pop_res(), Some((node_id, event(Event::MapEvent(key, MapEvent::OnSet(sub_key, node_id, value))))));
    assert_eq!(sim.pop_res(), None);
}

#[test]
fn single_node_sub_after() {
    let node_id = 1;
    let mut sim = NetworkSimulator::<(), ()>::new(0);
    sim.add_node(TestNode::new(node_id, 1234));

    sim.process(100);

    let key = Map(1000);
    let sub_key = Key(2000);
    let value = vec![1, 2, 3, 4];

    sim.control(node_id, control(Control::MapCmd(key, MapControl::Set(sub_key, value.clone()))));
    sim.process(100);

    assert_eq!(sim.pop_res(), None);

    sim.control(node_id, control(Control::MapCmd(key, MapControl::Sub)));
    sim.process(100);

    assert_eq!(sim.pop_res(), Some((node_id, event(Event::MapEvent(key, MapEvent::OnSet(sub_key, node_id, value))))));
    assert_eq!(sim.pop_res(), None);
}

#[test]
fn dht_kv_two_nodes() {
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

    let key = Map(1);
    let sub_key = Key(2000);
    let value = vec![1, 2, 3, 4];

    sim.control(node1, control(Control::MapCmd(key, MapControl::Sub)));
    sim.process(100);

    sim.control(node2, control(Control::MapCmd(key, MapControl::Set(sub_key, value.clone()))));
    sim.process(100);

    assert_eq!(sim.pop_res(), Some((node1, event(Event::MapEvent(key, MapEvent::OnSet(sub_key, node2, value))))));
    assert_eq!(sim.pop_res(), None);
}

#[test]
fn dht_kv_two_nodes_sub_after() {
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

    let key = Map(1);
    let sub_key = Key(2000);
    let value = vec![1, 2, 3, 4];

    sim.control(node2, control(Control::MapCmd(key, MapControl::Set(sub_key, value.clone()))));
    sim.process(100);

    assert_eq!(sim.pop_res(), None);

    sim.control(node1, control(Control::MapCmd(key, MapControl::Sub)));
    sim.process(100);

    assert_eq!(sim.pop_res(), Some((node1, event(Event::MapEvent(key, MapEvent::OnSet(sub_key, node2, value))))));
    assert_eq!(sim.pop_res(), None);
}

#[test]
fn dht_kv_move_key_other_relay() {
    let node1 = 1;
    let node2 = 2;
    let mut sim = NetworkSimulator::<(), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234));
    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    let key = Map(2);
    let sub_key = Key(2000);
    let value = vec![1, 2, 3, 4];

    sim.control(node1, control(Control::MapCmd(key, MapControl::Sub)));
    sim.process(100);

    sim.control(node1, control(Control::MapCmd(key, MapControl::Set(sub_key, value.clone()))));
    sim.process(100);

    assert_eq!(sim.pop_res(), Some((node1, event(Event::MapEvent(key, MapEvent::OnSet(sub_key, node1, value))))));
    assert_eq!(sim.pop_res(), None);

    let addr2 = sim.add_node(TestNode::new(node2, 1235));
    sim.control(node1, ExtIn::ConnectTo(addr2));

    // For sync
    for _i in 0..10 {
        sim.process(500);
    }

    assert_eq!(sim.pop_res(), None);
}

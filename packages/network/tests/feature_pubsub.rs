use atm0s_sdn_network::{
    features::{
        pubsub::{ChannelControl, ChannelEvent, ChannelId, Control, Event},
        FeaturesControl, FeaturesEvent,
    },
    ExtIn, ExtOut,
};

use crate::simulator::{NetworkSimulator, TestNode};

mod simulator;

fn control(control: Control) -> ExtIn<()> {
    ExtIn::FeaturesControl(FeaturesControl::PubSub(control))
}

fn event(event: Event) -> ExtOut<()> {
    ExtOut::FeaturesEvent(FeaturesEvent::PubSub(event))
}

#[test]
fn feature_pubsub_single_node() {
    let node_id = 1;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);
    sim.add_node(TestNode::new(node_id, 1234, vec![]));

    sim.process(100);

    let channel = ChannelId(1000);
    let value = vec![1, 2, 3, 4];

    sim.control(node_id, control(Control(channel, ChannelControl::SubSource(node_id))));
    sim.control(node_id, control(Control(channel, ChannelControl::PubData(value.clone()))));
    sim.process(100);
    assert_eq!(sim.pop_res(), Some((node_id, event(Event(channel, ChannelEvent::SourceData(node_id, value))))));
    assert_eq!(sim.pop_res(), None);
}

#[test]
fn feature_pubsub_two_nodes() {
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

    let channel = ChannelId(1000);
    let value = vec![1, 2, 3, 4];

    sim.control(node1, control(Control(channel, ChannelControl::SubSource(node2))));
    sim.process(1);

    sim.control(node2, control(Control(channel, ChannelControl::PubData(value.clone()))));
    sim.process(1);
    assert_eq!(sim.pop_res(), Some((node1, event(Event(channel, ChannelEvent::SourceData(node2, value))))));
    assert_eq!(sim.pop_res(), None);
}

#[test]
fn feature_pubsub_three_nodes() {
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

    let channel = ChannelId(1000);
    let value = vec![1, 2, 3, 4];

    sim.control(node1, control(Control(channel, ChannelControl::SubSource(node3))));
    sim.process(1);

    sim.control(node3, control(Control(channel, ChannelControl::PubData(value.clone()))));
    sim.process(1);
    assert_eq!(sim.pop_res(), Some((node1, event(Event(channel, ChannelEvent::SourceData(node3, value))))));
    assert_eq!(sim.pop_res(), None);
}

use atm0s_sdn_network::{
    features::{
        dht_kv::{Control, Event, Map, MapControl, MapEvent, Key},
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
}

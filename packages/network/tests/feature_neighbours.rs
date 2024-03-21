use atm0s_sdn_identity::ConnId;
use atm0s_sdn_network::{
    features::{neighbours, FeaturesControl, FeaturesEvent},
    ExtIn, ExtOut,
};

use crate::simulator::{NetworkSimulator, TestNode};

mod simulator;

#[test]
fn feature_neighbours_two_nodes() {
    let node1 = 1;
    let node2 = 2;
    let mut sim = NetworkSimulator::<(), (), (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![]));
    let addr2 = sim.add_node(TestNode::new(node2, 1235, vec![]));

    sim.control(node1, ExtIn::FeaturesControl(FeaturesControl::Neighbours(neighbours::Control::Sub)));
    sim.control(node2, ExtIn::FeaturesControl(FeaturesControl::Neighbours(neighbours::Control::Sub)));

    sim.control(node1, ExtIn::ConnectTo(addr2));

    // For sync
    for _i in 0..4 {
        sim.process(500);
    }

    assert_eq!(
        sim.pop_res(),
        Some((node1, ExtOut::FeaturesEvent(FeaturesEvent::Neighbours(neighbours::Event::Connected(node2, ConnId::from_out(0, 1000))))))
    );
    assert_eq!(
        sim.pop_res(),
        Some((node2, ExtOut::FeaturesEvent(FeaturesEvent::Neighbours(neighbours::Event::Connected(node1, ConnId::from_in(0, 1000))))))
    );
}

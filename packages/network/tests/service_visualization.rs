use std::sync::Arc;

use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    services::visualization::{self, ConnectionInfo, Control, Event, VisualizationServiceBuilder},
    ExtIn, ExtOut,
};

use crate::simulator::{node_to_addr, NetworkSimulator, TestNode};

mod simulator;

fn node_changed(node: NodeId, remotes: &[(NodeId, ConnId)]) -> ExtOut<Event> {
    ExtOut::ServicesEvent(Event::NodeChanged(
        node,
        remotes
            .iter()
            .map(|(n, c)| ConnectionInfo {
                conn: *c,
                dest: *n,
                remote: node_to_addr(*n),
                rtt_ms: 0,
            })
            .collect(),
    ))
}

#[test]
fn service_visualization_simple() {
    let node1 = 1;
    let node2 = 2;
    let mut sim = NetworkSimulator::<Control, Event, (), ()>::new(0);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![Arc::new(VisualizationServiceBuilder::new(true, node1))]));
    let addr2 = sim.add_node(TestNode::new(node2, 1235, vec![Arc::new(VisualizationServiceBuilder::new(false, node2))]));

    sim.control(node1, ExtIn::ServicesControl(visualization::SERVICE_ID.into(), Control::Subscribe));

    // For sync, snapshot and subscribe process
    for _i in 0..6 {
        sim.process(1000);
    }

    assert_eq!(sim.pop_res(), Some((node1, ExtOut::ServicesEvent(Event::GotAll(vec![])))));
    assert_eq!(sim.pop_res(), Some((node1, ExtOut::ServicesEvent(Event::NodeChanged(node1, vec![])))));

    sim.control(node1, ExtIn::ConnectTo(addr2));

    // For sync route table and snapshot process
    for _i in 0..5 {
        sim.process(1000);
    }

    assert_eq!(sim.pop_res(), Some((node1, node_changed(node1, &[(node2, ConnId::from_out(0, 1000))]))));

    assert_eq!(sim.pop_res(), Some((node1, node_changed(node2, &[(node1, ConnId::from_in(0, 1000))]))));
}

/// 3 nodes: Master <--> Master <--> Agent
///
/// All collectors should receive the same information about the agents
///
#[test]
fn service_visualization_multi_collectors() {
    let node1 = 1;
    let node2 = 2;
    let node3 = 3;
    let mut sim = NetworkSimulator::<Control, Event, (), ()>::new(0);
    sim.enable_log(log::LevelFilter::Debug);

    let _addr1 = sim.add_node(TestNode::new(node1, 1234, vec![Arc::new(VisualizationServiceBuilder::new(true, node1))]));
    let addr2 = sim.add_node(TestNode::new(node2, 1235, vec![Arc::new(VisualizationServiceBuilder::new(true, node2))]));
    let addr3 = sim.add_node(TestNode::new(node3, 1236, vec![Arc::new(VisualizationServiceBuilder::new(false, node3))]));

    sim.control(node1, ExtIn::ServicesControl(visualization::SERVICE_ID.into(), Control::Subscribe));
    sim.control(node2, ExtIn::ServicesControl(visualization::SERVICE_ID.into(), Control::Subscribe));

    // For sync, snapshot and subscribe process
    for _i in 0..6 {
        sim.process(1000);
    }

    assert_eq!(sim.pop_res(), Some((node1, ExtOut::ServicesEvent(Event::GotAll(vec![])))));
    assert_eq!(sim.pop_res(), Some((node2, ExtOut::ServicesEvent(Event::GotAll(vec![])))));
    assert_eq!(sim.pop_res(), Some((node1, ExtOut::ServicesEvent(Event::NodeChanged(node1, vec![])))));
    assert_eq!(sim.pop_res(), Some((node2, ExtOut::ServicesEvent(Event::NodeChanged(node2, vec![])))));

    sim.control(node1, ExtIn::ConnectTo(addr2));
    sim.control(node2, ExtIn::ConnectTo(addr3));

    // For sync route table and snapshot process
    for _i in 0..5 {
        sim.process(1000);
    }

    let mut node1_events = vec![];
    let mut node2_events = vec![];

    while let Some((node, e)) = sim.pop_res() {
        if node == node1 {
            node1_events.push(e);
        } else if node == node2 {
            node2_events.push(e);
        } else {
            panic!("Unexpected node: {}", node);
        }
    }

    assert_eq!(
        node1_events,
        vec![
            node_changed(node1, &[(node2, ConnId::from_out(0, 1000))]),
            node_changed(node2, &[(node3, ConnId::from_out(0, 1000)), (node1, ConnId::from_in(0, 1000))]),
            node_changed(node3, &[(node2, ConnId::from_in(0, 1000))]),
        ]
    );

    assert_eq!(
        node2_events,
        vec![
            node_changed(node1, &[(node2, ConnId::from_out(0, 1000))]),
            node_changed(node2, &[(node3, ConnId::from_out(0, 1000)), (node1, ConnId::from_in(0, 1000))]),
            node_changed(node3, &[(node2, ConnId::from_in(0, 1000))]),
        ]
    );
}

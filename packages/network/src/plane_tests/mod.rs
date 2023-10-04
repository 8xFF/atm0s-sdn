use std::{collections::VecDeque, sync::Arc};

use bluesea_identity::{ConnId, NodeId};
use parking_lot::{Mutex, RwLock};

use crate::{
    internal::{CrossHandlerEvent, CrossHandlerGate},
    mock::{MockOutput, MockTransportConnector},
    msg::TransportMsg,
    BehaviorAgent, CrossHandlerRoute,
};

mod behavior_close;
mod behavior_local_event;
mod behavior_local_msg;
mod behavior_to_handler;
mod handler_to_handler;
mod simple_network;

#[derive(Debug, PartialEq, Eq)]
pub enum CrossHandlerGateMockEvent<BE, HE> {
    CloseConn(ConnId),
    CloseNode(NodeId),
    SendToBehaviour(u8, BE),
    SendToHandlerFromBehaviour(u8, CrossHandlerRoute, HE),
    SendToHandlerFromHandler(u8, NodeId, ConnId, CrossHandlerRoute, HE),
    SentToNet(TransportMsg),
}

pub struct CrossHandlerGateMock<BE, HE> {
    events: Arc<Mutex<VecDeque<CrossHandlerGateMockEvent<BE, HE>>>>,
}

impl<BE, HE> Default for CrossHandlerGateMock<BE, HE> {
    fn default() -> Self {
        Self {
            events: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<BE, HE> CrossHandlerGate<BE, HE> for CrossHandlerGateMock<BE, HE>
where
    BE: Send + Sync,
    HE: Send + Sync,
{
    fn close_conn(&self, conn: ConnId) {
        self.events.lock().push_back(CrossHandlerGateMockEvent::CloseConn(conn));
    }

    fn close_node(&self, node: NodeId) {
        self.events.lock().push_back(CrossHandlerGateMockEvent::CloseNode(node));
    }

    fn send_to_behaviour(&self, service_id: u8, event: BE) -> Option<()> {
        self.events.lock().push_back(CrossHandlerGateMockEvent::SendToBehaviour(service_id, event));
        Some(())
    }

    fn send_to_handler(&self, service_id: u8, route: CrossHandlerRoute, event: CrossHandlerEvent<HE>) -> Option<()> {
        match event {
            CrossHandlerEvent::FromBehavior(event) => {
                self.events.lock().push_back(CrossHandlerGateMockEvent::SendToHandlerFromBehaviour(service_id, route, event));
            }
            CrossHandlerEvent::FromHandler(node, conn, event) => {
                self.events.lock().push_back(CrossHandlerGateMockEvent::SendToHandlerFromHandler(service_id, node, conn, route, event));
            }
        }
        Some(())
    }

    fn send_to_net(&self, msg: crate::msg::TransportMsg) -> Option<()> {
        self.events.lock().push_back(CrossHandlerGateMockEvent::SentToNet(msg));
        Some(())
    }
}

pub fn create_mock_behaviour_agent<BE: Send + Sync + 'static, HE: Send + Sync + 'static>(
    local_node_id: NodeId,
    service_id: u8,
) -> (BehaviorAgent<BE, HE>, Arc<Mutex<VecDeque<MockOutput>>>, Arc<Mutex<VecDeque<CrossHandlerGateMockEvent<BE, HE>>>>) {
    let connector = Arc::new(MockTransportConnector::default());
    let connector_output = connector.output.clone();

    let cross_gate = Arc::new(RwLock::new(CrossHandlerGateMock::<BE, HE>::default()));
    let cross_gate_events = cross_gate.read().events.clone();

    let agent = BehaviorAgent::<BE, HE> {
        service_id,
        local_node_id,
        connector,
        cross_gate,
    };

    (agent, connector_output, cross_gate_events)
}

use std::collections::VecDeque;

use crate::msg::{DiscoveryBehaviorEvent, DiscoveryHandlerEvent};
use crate::DiscoveryMsg;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::behaviour::{ConnectionContext, ConnectionHandler, ConnectionHandlerAction};
use atm0s_sdn_network::transport::ConnectionEvent;

pub struct DiscoveryConnectionHandler<BE, HE> {
    outputs: VecDeque<ConnectionHandlerAction<BE, HE>>,
}

impl<BE, HE> DiscoveryConnectionHandler<BE, HE> {
    pub fn new() -> Self {
        Self { outputs: VecDeque::new() }
    }
}

#[allow(unused_variables)]
impl<BE, HE> ConnectionHandler<BE, HE> for DiscoveryConnectionHandler<BE, HE>
where
    BE: TryInto<DiscoveryBehaviorEvent> + From<DiscoveryBehaviorEvent> + Send + Sync + 'static,
    HE: TryInto<DiscoveryHandlerEvent> + From<DiscoveryHandlerEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, ctx: &ConnectionContext, now_ms: u64) {}

    fn on_tick(&mut self, ctx: &ConnectionContext, now_ms: u64, interval_ms: u64) {}

    fn on_awake(&mut self, ctx: &ConnectionContext, now_ms: u64) {}

    fn on_event(&mut self, ctx: &ConnectionContext, now_ms: u64, event: ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => {
                if let Ok(msg) = msg.get_payload_bincode::<DiscoveryMsg>() {
                    self.outputs.push_back(ConnectionHandlerAction::ToBehaviour(DiscoveryBehaviorEvent::OnNetworkMessage(msg).into()));
                }
            }
            ConnectionEvent::Stats(_) => {}
        }
    }

    fn on_other_handler_event(&mut self, ctx: &ConnectionContext, now_ms: u64, from_node: NodeId, from_conn: ConnId, event: HE) {}

    fn on_behavior_event(&mut self, ctx: &ConnectionContext, now_ms: u64, event: HE) {}

    fn on_closed(&mut self, ctx: &ConnectionContext, now_ms: u64) {}

    fn pop_action(&mut self) -> Option<ConnectionHandlerAction<BE, HE>> {
        self.outputs.pop_front()
    }
}

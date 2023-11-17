use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::{
    behaviour::{ConnectionContext, ConnectionHandler},
    transport::ConnectionEvent,
};

use crate::{
    msg::{PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{feedback::Feedback, ChannelIdentify, PubsubRelay},
};

pub const CONTROL_META_TYPE: u8 = 1;
pub const FEEDBACK_TYPE: u8 = 2;

pub struct PubsubServiceConnectionHandler {
    pub(crate) node_id: NodeId,
    pub(crate) relay: PubsubRelay,
}

impl<BE, HE> ConnectionHandler<BE, HE> for PubsubServiceConnectionHandler
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_tick(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _interval_ms: u64) {}

    fn on_awake(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn on_event(&mut self, ctx: &ConnectionContext, now_ms: u64, event: atm0s_sdn_network::transport::ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => match msg.header.meta {
                CONTROL_META_TYPE => {
                    if let Ok(cmd) = msg.get_payload_bincode::<PubsubRemoteEvent>() {
                        self.relay.on_event(now_ms, ctx.remote_node_id, ctx.conn_id, cmd);
                    }
                }
                FEEDBACK_TYPE => {
                    if let Ok(fb) = msg.get_payload_bincode::<Feedback>() {
                        self.relay.on_feedback(now_ms, fb.channel, ctx.remote_node_id, ctx.conn_id, fb);
                    }
                }
                _ => {
                    if let Some(from_node) = msg.header.from_node {
                        log::trace!(
                            "[PubsubServiceConnectionHandler {}] on remote channel data from {} with channel uuid {}",
                            self.node_id,
                            from_node,
                            msg.header.stream_id
                        );
                        self.relay.relay(ChannelIdentify::new(msg.header.stream_id, from_node), msg);
                    }
                }
            },
            ConnectionEvent::Stats(_msg) => {}
        }
    }

    fn on_other_handler_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _from_node: atm0s_sdn_identity::NodeId, _from_conn: atm0s_sdn_identity::ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _ctx: &ConnectionContext, _now_ms: u64, _event: HE) {}

    fn on_closed(&mut self, _ctx: &ConnectionContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<atm0s_sdn_network::behaviour::ConnectionHandlerAction<BE, HE>> {
        None
    }
}

#[cfg(test)]
mod tests {}

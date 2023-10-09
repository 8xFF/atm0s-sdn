use bluesea_identity::NodeId;
use network::{behaviour::ConnectionHandler, transport::ConnectionEvent};

use crate::{
    msg::{PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{feedback::Feedback, ChannelIdentify, PubsubRelay},
};

pub const CONTROL_META_TYPE: u8 = 1;
pub const FEEDBACK_TYPE: u8 = 2;

pub struct PubsubServiceConnectionHandler<BE, HE> {
    pub(crate) node_id: NodeId,
    pub(crate) relay: PubsubRelay<BE, HE>,
}

impl<BE, HE> ConnectionHandler<BE, HE> for PubsubServiceConnectionHandler<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &network::ConnectionAgent<BE, HE>) {
        self.relay.on_connection_opened(agent)
    }

    fn on_tick(&mut self, _agent: &network::ConnectionAgent<BE, HE>, _ts_ms: u64, _interval_ms: u64) {}

    fn on_event(&mut self, agent: &network::ConnectionAgent<BE, HE>, event: network::transport::ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => match msg.header.meta {
                CONTROL_META_TYPE => {
                    if let Ok(cmd) = msg.get_payload_bincode::<PubsubRemoteEvent>() {
                        self.relay.on_event(agent.remote_node_id(), agent.conn_id(), cmd);
                    }
                }
                FEEDBACK_TYPE => {
                    if let Ok(fb) = msg.get_payload_bincode::<Feedback>() {
                        self.relay.on_feedback(fb.channel, agent.remote_node_id(), agent.conn_id(), fb);
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

    fn on_other_handler_event(&mut self, _agent: &network::ConnectionAgent<BE, HE>, _from_node: bluesea_identity::NodeId, _from_conn: bluesea_identity::ConnId, _event: HE) {}

    fn on_behavior_event(&mut self, _agent: &network::ConnectionAgent<BE, HE>, _event: HE) {}

    fn on_closed(&mut self, agent: &network::ConnectionAgent<BE, HE>) {
        self.relay.on_connection_closed(agent)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn should_send_event_to_logic() {}
}

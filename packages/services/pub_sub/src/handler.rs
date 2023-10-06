use network::{behaviour::ConnectionHandler, transport::ConnectionEvent};

use crate::{
    msg::{PubsubRemoteEvent, PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{ChannelIdentify, PubsubRelay},
};

pub const CONTROL_META_TYPE: u8 = 1;

pub struct PubsubServiceConnectionHandler<BE, HE> {
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

    fn on_tick(&mut self, agent: &network::ConnectionAgent<BE, HE>, ts_ms: u64, interval_ms: u64) {}

    fn on_event(&mut self, agent: &network::ConnectionAgent<BE, HE>, event: network::transport::ConnectionEvent) {
        match event {
            ConnectionEvent::Msg(msg) => match msg.header.meta {
                CONTROL_META_TYPE => {
                    if let Ok(cmd) = msg.get_payload_bincode::<PubsubRemoteEvent>() {
                        self.relay.on_event(agent.remote_node_id(), agent.conn_id(), cmd);
                    }
                }
                _ => {
                    if let Some(from_node) = msg.header.from_node {
                        self.relay.relay(ChannelIdentify::new(from_node, msg.header.stream_id), msg);
                    }
                }
            },
            ConnectionEvent::Stats(_msg) => {}
        }
    }

    fn on_other_handler_event(&mut self, agent: &network::ConnectionAgent<BE, HE>, from_node: bluesea_identity::NodeId, from_conn: bluesea_identity::ConnId, event: HE) {}

    fn on_behavior_event(&mut self, agent: &network::ConnectionAgent<BE, HE>, event: HE) {}

    fn on_closed(&mut self, agent: &network::ConnectionAgent<BE, HE>) {
        self.relay.on_connection_closed(agent)
    }
}

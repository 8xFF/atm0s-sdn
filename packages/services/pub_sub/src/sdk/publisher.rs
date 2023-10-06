use std::sync::Arc;

use bluesea_router::RouteRule;
use bytes::Bytes;
use network::msg::{MsgHeader, TransportMsg};
use parking_lot::RwLock;

use crate::{
    msg::{PubsubServiceBehaviourEvent, PubsubServiceHandlerEvent},
    relay::{local::LocalRelay, logic::PubsubRelayLogic, remote::RemoteRelay, ChannelIdentify, LocalPubId},
    PUBSUB_SERVICE_ID,
};

pub struct Publisher<BE, HE> {
    uuid: LocalPubId,
    channel: ChannelIdentify,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    remote: Arc<RwLock<RemoteRelay<BE, HE>>>,
    local: Arc<RwLock<LocalRelay>>,
}

impl<BE, HE> Publisher<BE, HE>
where
    BE: From<PubsubServiceBehaviourEvent> + TryInto<PubsubServiceBehaviourEvent> + Send + Sync + 'static,
    HE: From<PubsubServiceHandlerEvent> + TryInto<PubsubServiceHandlerEvent> + Send + Sync + 'static,
{
    pub fn new(uuid: LocalPubId, channel: ChannelIdentify, logic: Arc<RwLock<PubsubRelayLogic>>, remote: Arc<RwLock<RemoteRelay<BE, HE>>>, local: Arc<RwLock<LocalRelay>>) -> Self {
        Self { uuid, channel, logic, remote, local }
    }

    pub fn send(&self, data: Bytes) {
        if let Some((remotes, locals)) = self.logic.read().relay(self.channel) {
            if remotes.len() > 0 {
                let mut header = MsgHeader::build_reliable(PUBSUB_SERVICE_ID, RouteRule::Direct, self.channel.uuid());
                header.from_node = Some(self.channel.source());
                let msg = TransportMsg::build_raw(header, &data);
                self.remote.read().relay(remotes, &msg);
            }

            self.local.read().relay(locals, data);
        }
    }
}

impl<BE, HE> Drop for Publisher<BE, HE> {
    fn drop(&mut self) {}
}

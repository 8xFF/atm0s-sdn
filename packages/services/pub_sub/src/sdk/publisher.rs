use std::sync::Arc;

use bluesea_router::RouteRule;
use bytes::Bytes;
use network::msg::{MsgHeader, TransportMsg};
use parking_lot::RwLock;

use crate::{
    relay::{feedback::Feedback, local::LocalRelay, logic::PubsubRelayLogic, remote::RemoteRelay, ChannelIdentify, LocalPubId},
    PUBSUB_SERVICE_ID,
};

pub struct Publisher {
    uuid: LocalPubId,
    channel: ChannelIdentify,
    logic: Arc<RwLock<PubsubRelayLogic>>,
    remote: Arc<RwLock<RemoteRelay>>,
    local: Arc<RwLock<LocalRelay>>,
    fb_rx: async_std::channel::Receiver<Feedback>,
}

impl Publisher {
    pub fn new(uuid: LocalPubId, channel: ChannelIdentify, logic: Arc<RwLock<PubsubRelayLogic>>, remote: Arc<RwLock<RemoteRelay>>, local: Arc<RwLock<LocalRelay>>) -> Self {
        let (tx, rx) = async_std::channel::bounded(100);
        local.write().on_local_pub(channel.uuid(), uuid, tx);

        Self {
            uuid,
            channel,
            logic,
            remote,
            local,
            fb_rx: rx,
        }
    }

    pub fn identify(&self) -> ChannelIdentify {
        self.channel
    }

    pub fn send(&self, data: Bytes) {
        if let Some((remotes, locals)) = self.logic.read().relay(self.channel) {
            if remotes.len() > 0 {
                let mut header = MsgHeader::build_reliable(PUBSUB_SERVICE_ID, RouteRule::Direct, self.channel.uuid());
                header.from_node = Some(self.channel.source());
                let msg = TransportMsg::build_raw(header, &data);
                self.remote.read().relay(remotes, &msg);
            }

            self.local.read().relay(self.channel.source(), self.channel.uuid(), locals, data);
        }
    }

    pub async fn recv_feedback(&self) -> Option<Feedback> {
        self.fb_rx.recv().await.ok()
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.local.write().on_local_unpub(self.channel.uuid(), self.uuid);
    }
}

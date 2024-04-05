use super::ChannelId;

pub struct NetworkLogic {}

impl NetworkLogic {
    pub fn new() -> Self {
        Self {}
    }

    pub fn on_subscribe(&mut self, channel: ChannelId) {}

    pub fn on_publish(&mut self, channel: ChannelId) {}

    pub fn on_unsubscribe(&mut self, channel: ChannelId) {}
}

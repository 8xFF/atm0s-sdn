use super::{msg::PubsubMessage, ChannelControl};

pub enum ChannelOutput {}

pub struct Channel {}

impl Channel {
    pub fn on_tick(&mut self, now: u64) {}

    pub fn on_control(&mut self, now: u64, control: ChannelControl) {}

    pub fn on_remote<'a>(&mut self, now: u64, msg: PubsubMessage<'a>) {}

    pub fn pop_output(&mut self) -> Option<ChannelOutput> {
        None
    }
}

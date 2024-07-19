use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    time::Instant,
};

use atm0s_sdn::features::pubsub::{self, ChannelControl, Feedback};
use str0m::media::KeyframeRequestKind;

use super::{TrackMedia, WhepOwner, WhipOwner};

pub fn room_channel(room: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    room.hash(&mut hasher);
    hasher.finish()
}

pub enum Input {
    Pubsub(pubsub::Event),
    WhipStart(WhipOwner, String),
    WhipStop(WhipOwner),
    WhipMedia(WhipOwner, TrackMedia),
    WhepStart(WhepOwner, String),
    WhepStop(WhepOwner),
    WhepRequest(WhepOwner, KeyframeRequestKind),
}

pub enum Output {
    Pubsub(pubsub::Control),
    WhepMedia(Vec<WhepOwner>, TrackMedia),
    WhipControl(Vec<WhipOwner>, KeyframeRequestKind),
}

pub struct Channel {
    whips: Vec<WhipOwner>,
    wheps: Vec<WhepOwner>,
}

#[derive(Default)]
pub struct ClusterLogic {
    channels: HashMap<u64, Channel>,
    whips: HashMap<WhipOwner, u64>,
    wheps: HashMap<WhepOwner, u64>,
}

impl ClusterLogic {
    pub fn on_input(&mut self, now: Instant, input: Input) -> Option<Output> {
        match input {
            Input::Pubsub(pubsub::Event(channel, event)) => match event {
                pubsub::ChannelEvent::RouteChanged(_) => None,
                pubsub::ChannelEvent::SourceData(_, data) => {
                    let pkt = TrackMedia::from_buffer(&data);
                    let channel = self.channels.get(&channel)?;
                    Some(Output::WhepMedia(channel.wheps.clone(), pkt))
                }
                pubsub::ChannelEvent::FeedbackData(fb) => {
                    let channel = self.channels.get(&channel)?;
                    let kind = match fb.kind {
                        0 => KeyframeRequestKind::Pli,
                        _ => KeyframeRequestKind::Fir,
                    };
                    Some(Output::WhipControl(channel.whips.clone(), kind))
                }
            },
            Input::WhipStart(owner, room) => {
                log::info!("WhipStart: {:?}, {:?}", owner, room);
                let channel_id = room_channel(&room);
                self.whips.insert(owner, channel_id);
                let channel = self.channels.entry(channel_id).or_insert(Channel { whips: Vec::new(), wheps: Vec::new() });
                channel.whips.push(owner);
                if channel.whips.len() == 1 {
                    Some(Output::Pubsub(pubsub::Control(channel_id.into(), pubsub::ChannelControl::PubStart)))
                } else {
                    None
                }
            }
            Input::WhipStop(owner) => {
                log::info!("WhipStop: {:?}", owner);
                let channel_id = self.whips.remove(&owner)?;
                let channel = self.channels.get_mut(&channel_id)?;
                channel.whips.retain(|&o| o != owner);
                if channel.whips.is_empty() {
                    Some(Output::Pubsub(pubsub::Control(channel_id.into(), pubsub::ChannelControl::PubStop)))
                } else {
                    None
                }
            }
            Input::WhipMedia(owner, media) => {
                log::trace!("WhipMedia: {:?}, {}", owner, media.seq_no);
                let channel_id = self.whips.get(&owner)?;
                let buf = media.to_buffer();
                Some(Output::Pubsub(pubsub::Control((*channel_id).into(), pubsub::ChannelControl::PubData(buf))))
            }
            Input::WhepStart(owner, room) => {
                log::info!("WhepStart: {:?}, {:?}", owner, room);
                let channel_id = room_channel(&room);
                self.wheps.insert(owner, channel_id);
                let channel = self.channels.entry(channel_id).or_insert(Channel { whips: Vec::new(), wheps: Vec::new() });
                channel.wheps.push(owner);
                if channel.wheps.len() == 1 {
                    Some(Output::Pubsub(pubsub::Control(channel_id.into(), pubsub::ChannelControl::SubAuto)))
                } else {
                    None
                }
            }
            Input::WhepStop(owner) => {
                log::info!("WhepStop: {:?}", owner);
                let channel_id = self.wheps.remove(&owner)?;
                let channel = self.channels.get_mut(&channel_id)?;
                channel.wheps.retain(|&o| o != owner);
                if channel.wheps.is_empty() {
                    Some(Output::Pubsub(pubsub::Control(channel_id.into(), pubsub::ChannelControl::UnsubAuto)))
                } else {
                    None
                }
            }
            Input::WhepRequest(owner, kind) => {
                let kind = match kind {
                    KeyframeRequestKind::Pli => 0,
                    KeyframeRequestKind::Fir => 1,
                };
                let channel_id = self.wheps.get(&owner)?;
                Some(Output::Pubsub(pubsub::Control(
                    (*channel_id).into(),
                    ChannelControl::FeedbackAuto(Feedback::simple(kind, 1, 1000, 2000)),
                )))
            }
        }
    }
}

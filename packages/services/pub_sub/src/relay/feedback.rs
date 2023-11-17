use std::collections::HashMap;

use atm0s_sdn_identity::ConnId;
use serde::{Deserialize, Serialize};

use crate::ChannelIdentify;

mod number;
mod passthrough;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NumberInfo {
    pub count: u64,
    pub sum: i64,
    pub max: i64,
    pub min: i64,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub enum FeedbackType {
    Passthrough(Vec<u8>),
    Number { window_ms: u32, info: NumberInfo },
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum FeedbackConsumerId {
    Local(u64),
    Remote(ConnId),
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Feedback {
    pub channel: ChannelIdentify,
    pub id: u8,
    pub feedback_type: FeedbackType,
}

pub(crate) trait SingleFeedbackProcessor: Send + Sync {
    fn on_tick(&mut self, now_ms: u64) -> Option<FeedbackType>;
    fn on_remove(&mut self, consumer_id: FeedbackConsumerId);
    fn on_feedback(&mut self, now_ms: u64, consumer_id: FeedbackConsumerId, fb: FeedbackType) -> Option<FeedbackType>;
}

pub struct ChannelFeedbackProcessor {
    channel: ChannelIdentify,
    types: HashMap<u8, Box<dyn SingleFeedbackProcessor>>,
}

impl ChannelFeedbackProcessor {
    pub fn new(channel: ChannelIdentify) -> Self {
        Self { channel, types: HashMap::new() }
    }

    pub fn on_tick(&mut self, now_ms: u64) -> Option<Vec<Feedback>> {
        let mut fbs = Vec::new();
        for (id, processor) in self.types.iter_mut() {
            if let Some(fb) = processor.on_tick(now_ms) {
                fbs.push(Feedback {
                    channel: self.channel,
                    id: *id,
                    feedback_type: fb,
                });
            }
        }
        if fbs.is_empty() {
            None
        } else {
            Some(fbs)
        }
    }

    pub fn on_unsub(&mut self, consumer_id: FeedbackConsumerId) {
        for (_, processor) in self.types.iter_mut() {
            processor.on_remove(consumer_id.clone());
        }
    }

    pub fn on_feedback(&mut self, now_ms: u64, consumer_id: FeedbackConsumerId, fb: Feedback) -> Option<Feedback> {
        let res = if let Some(processor) = self.types.get_mut(&fb.id) {
            processor.on_feedback(now_ms, consumer_id, fb.feedback_type)
        } else {
            let mut processor: Box<dyn SingleFeedbackProcessor> = match &fb.feedback_type {
                FeedbackType::Passthrough(_) => Box::new(passthrough::PassthroughFeedbackProcessor()),
                FeedbackType::Number { window_ms, info: _ } => Box::new(number::NumberFeedbackProcessor::new(*window_ms)),
            };

            let res = processor.on_feedback(now_ms, consumer_id, fb.feedback_type);
            self.types.insert(fb.id, processor);
            res
        };

        res.map(|new_fb| Feedback {
            channel: self.channel,
            id: fb.id,
            feedback_type: new_fb,
        })
    }
}

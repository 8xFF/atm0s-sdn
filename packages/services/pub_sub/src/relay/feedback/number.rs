use std::collections::HashMap;

use super::{FeedbackConsumerId, FeedbackType, NumberInfo, SingleFeedbackProcessor};

pub struct NumberFeedbackProcessor {
    window_ms: u32,
    fb_map: HashMap<FeedbackConsumerId, NumberInfo>,
    last_fb: u64,
    has_changed: bool,
}

impl NumberFeedbackProcessor {
    pub fn new(window_ms: u32) -> Self {
        Self {
            window_ms,
            fb_map: Default::default(),
            last_fb: 0,
            has_changed: false,
        }
    }

    fn sumary(&self) -> NumberInfo {
        let mut count = 0;
        let mut sum = 0;
        let mut max = i64::MIN;
        let mut min = i64::MAX;
        for (_, info) in self.fb_map.iter() {
            count += info.count;
            sum += info.sum;
            max = max.max(info.max);
            min = min.min(info.min);
        }
        NumberInfo { count, sum, max, min }
    }

    fn sumary_if_need(&mut self, now_ms: u64) -> Option<FeedbackType> {
        if self.last_fb + (self.window_ms as u64) <= now_ms && self.has_changed {
            self.last_fb = now_ms;
            self.has_changed = false;
            Some(FeedbackType::Number {
                window_ms: self.window_ms,
                info: self.sumary(),
            })
        } else {
            None
        }
    }
}

impl SingleFeedbackProcessor for NumberFeedbackProcessor {
    fn on_tick(&mut self, now_ms: u64) -> Option<FeedbackType> {
        self.sumary_if_need(now_ms)
    }

    fn on_remove(&mut self, consumer_id: FeedbackConsumerId) {
        self.fb_map.remove(&consumer_id);
    }

    fn on_feedback(&mut self, now_ms: u64, consumer_id: FeedbackConsumerId, fb: FeedbackType) -> Option<FeedbackType> {
        match &fb {
            FeedbackType::Passthrough(_) => {
                panic!("Should not happend")
            }
            FeedbackType::Number { window_ms, info } => {
                self.has_changed = true;
                self.window_ms = *window_ms;
                self.fb_map.insert(consumer_id, info.clone());
                self.sumary_if_need(now_ms)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::feedback::FeedbackType;

    fn build_fb(count: u64, sum: i64, max: i64, min: i64) -> FeedbackType {
        FeedbackType::Number {
            window_ms: 1000,
            info: NumberInfo { count, sum, max, min },
        }
    }

    #[test]
    fn single() {
        let mut processor = NumberFeedbackProcessor::new(1000);
        assert_eq!(processor.on_feedback(2000, FeedbackConsumerId::Local(1), build_fb(1, 1, 1, 1)), Some(build_fb(1, 1, 1, 1)));
        assert_eq!(processor.on_feedback(2500, FeedbackConsumerId::Local(1), build_fb(2, 2, 2, 2)), None);
        assert_eq!(processor.on_tick(3000), Some(build_fb(2, 2, 2, 2)));
        assert_eq!(processor.on_tick(4000), None);
        assert_eq!(processor.on_feedback(4000, FeedbackConsumerId::Local(1), build_fb(3, 3, 3, 3)), Some(build_fb(3, 3, 3, 3)));
    }

    #[test]
    fn multi() {
        let mut processor = NumberFeedbackProcessor::new(1000);
        assert_eq!(processor.on_feedback(2000, FeedbackConsumerId::Local(1), build_fb(1, 1, 1, 1)), Some(build_fb(1, 1, 1, 1)));
        assert_eq!(processor.on_feedback(2500, FeedbackConsumerId::Local(2), build_fb(1, 2, 2, 2)), None);
        assert_eq!(processor.on_tick(3000), Some(build_fb(2, 3, 2, 1)));
        assert_eq!(processor.on_tick(4000), None);
        assert_eq!(processor.on_feedback(4000, FeedbackConsumerId::Local(1), build_fb(2, 3, 3, 3)), Some(build_fb(3, 5, 3, 2)));

        assert_eq!(processor.on_feedback(4500, FeedbackConsumerId::Local(1), build_fb(2, 3, 3, 3)), None);
        processor.on_remove(FeedbackConsumerId::Local(2));
        assert_eq!(processor.on_tick(5000), Some(build_fb(2, 3, 3, 3)));
    }
}

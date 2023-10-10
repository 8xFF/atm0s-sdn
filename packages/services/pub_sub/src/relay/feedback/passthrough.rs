use super::{FeedbackConsumerId, FeedbackType, SingleFeedbackProcessor};

pub struct PassthroughFeedbackProcessor();

impl SingleFeedbackProcessor for PassthroughFeedbackProcessor {
    fn on_tick(&mut self, _now_ms: u64) -> Option<FeedbackType> {
        None
    }

    fn on_remove(&mut self, _consumer_id: FeedbackConsumerId) {}

    fn on_feedback(&mut self, _now_ms: u64, _consumer_id: FeedbackConsumerId, fb: FeedbackType) -> Option<FeedbackType> {
        match &fb {
            FeedbackType::Passthrough(_) => Some(fb),
            FeedbackType::Number { window_ms: _, info: _ } => {
                panic!("Should not happend")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::feedback::FeedbackType;

    #[test]
    fn test_passthrough() {
        let mut processor = PassthroughFeedbackProcessor();
        for i in 0..10 {
            let fb = FeedbackType::Passthrough(vec![i]);
            assert_eq!(processor.on_feedback(0, FeedbackConsumerId::Local(1), fb.clone()), Some(fb));
        }
    }
}

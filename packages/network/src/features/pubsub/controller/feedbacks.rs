use std::{collections::VecDeque, net::SocketAddr};

use crate::{base::FeatureControlActor, features::pubsub::msg::Feedback};

#[derive(Debug, PartialEq, Eq)]
enum FeedbackSource {
    Local(FeatureControlActor),
    Remote(SocketAddr),
}

#[derive(Debug, Default)]
struct SingleFeedbackKind {
    kind: u8,
    feedbacks: Vec<(FeedbackSource, Feedback, u64)>,
    feedbacks_updated: bool,
    last_feedback_ts: Option<u64>,
}

impl SingleFeedbackKind {
    fn on_local_feedback(&mut self, now: u64, actor: FeatureControlActor, fb: Feedback) {
        self.feedbacks_updated = true;
        let source = FeedbackSource::Local(actor);
        if let Some(index) = self.feedbacks.iter().position(|(a, _, _)| *a == source) {
            self.feedbacks[index] = (source, fb, now);
        } else {
            self.feedbacks.push((source, fb, now));
        }
    }

    fn on_remote_feedback(&mut self, now: u64, remote: SocketAddr, fb: Feedback) {
        self.feedbacks_updated = true;
        let source = FeedbackSource::Remote(remote);
        if let Some(index) = self.feedbacks.iter().position(|(a, _, _)| *a == source) {
            self.feedbacks[index] = (source, fb, now);
        } else {
            self.feedbacks.push((source, fb, now));
        }
    }

    fn process_feedbacks(&mut self, now: u64) -> Option<Feedback> {
        if !self.feedbacks_updated {
            self.feedbacks.retain(|(_, fb, last_ts)| now < last_ts + fb.timeout_ms as u64);
            return None;
        }
        self.feedbacks_updated = false;
        log::debug!("[FeedbacksAggerator] on process feedback for kind {}", self.kind);
        let mut aggerated_fb: Option<Feedback> = None;
        for (_, fb, _) in &self.feedbacks {
            if let Some(aggerated_fb) = &mut aggerated_fb {
                *aggerated_fb = *aggerated_fb + *fb;
            } else {
                aggerated_fb = Some(*fb);
            }
        }
        self.feedbacks.retain(|(_, fb, last_ts)| now < last_ts + fb.timeout_ms as u64);

        if let Some(last_fb_ts) = self.last_feedback_ts {
            if let Some(fb) = &aggerated_fb {
                if now < last_fb_ts + fb.interval_ms as u64 {
                    return None;
                }
            }
        }
        aggerated_fb
    }
}

#[derive(Debug, Default)]
pub struct FeedbacksAggerator {
    feedbacks: Vec<SingleFeedbackKind>,
    queue: VecDeque<Feedback>,
}

impl FeedbacksAggerator {
    pub fn on_tick(&mut self, now: u64) {
        log::debug!("[FeedbacksAggerator] on tick {now}");
        self.process_feedbacks(now);
    }

    pub fn on_local_feedback(&mut self, now: u64, actor: FeatureControlActor, fb: Feedback) {
        log::debug!("[FeedbacksAggerator] on local_feedback from {:?} {:?}", actor, fb);
        let kind = self.get_fb_kind(fb.kind);
        kind.on_local_feedback(now, actor, fb);
        if let Some(fb) = kind.process_feedbacks(now) {
            self.queue.push_back(fb);
        }
    }

    pub fn on_remote_feedback(&mut self, now: u64, remote: SocketAddr, fb: Feedback) {
        log::debug!("[FeedbacksAggerator] on remote_feedback from {} {:?}", remote, fb);
        let kind = self.get_fb_kind(fb.kind);
        kind.on_remote_feedback(now, remote, fb);
        if let Some(fb) = kind.process_feedbacks(now) {
            self.queue.push_back(fb);
        }
    }

    pub fn pop_output(&mut self) -> Option<Feedback> {
        self.queue.pop_front()
    }

    fn process_feedbacks(&mut self, now: u64) {
        for kind in &mut self.feedbacks {
            while let Some(fb) = kind.process_feedbacks(now) {
                self.queue.push_back(fb);
            }
        }
        self.feedbacks.retain(|x| !x.feedbacks.is_empty());
    }

    fn get_fb_kind(&mut self, kind: u8) -> &mut SingleFeedbackKind {
        if let Some(index) = self.feedbacks.iter().position(|x| x.kind == kind) {
            &mut self.feedbacks[index]
        } else {
            let new = SingleFeedbackKind {
                kind,
                feedbacks: Vec::new(),
                feedbacks_updated: false,
                last_feedback_ts: None,
            };
            self.feedbacks.push(new);
            self.feedbacks.last_mut().expect("Should got last element")
        }
    }
}

#[cfg(test)]
mod test {
    use crate::base::FeatureControlActor;

    use super::{Feedback, FeedbacksAggerator};

    #[test]
    fn aggerator_single() {
        let mut aggerator = FeedbacksAggerator::default();
        let fb = Feedback::simple(0, 10, 1000, 2000);
        aggerator.on_local_feedback(1, FeatureControlActor::Controller, fb);
        aggerator.on_tick(2);
        assert_eq!(aggerator.pop_output(), Some(fb));
        assert_eq!(aggerator.pop_output(), None);
    }

    #[test]
    fn aggerator_single_rewrite() {
        let mut aggerator = FeedbacksAggerator::default();
        let fb = Feedback::simple(0, 10, 1000, 2000);
        aggerator.on_local_feedback(0, FeatureControlActor::Controller, fb);
        assert_eq!(aggerator.pop_output(), Some(fb));
        assert_eq!(aggerator.pop_output(), None);

        let fb = Feedback::simple(0, 11, 1000, 2000);
        aggerator.on_local_feedback(2, FeatureControlActor::Controller, fb);
        aggerator.on_tick(2000);
        assert_eq!(aggerator.pop_output(), Some(fb));
        assert_eq!(aggerator.pop_output(), None);
    }

    #[test]
    fn aggerator_multi_sources() {
        let mut aggerator = FeedbacksAggerator::default();
        let fb = Feedback::simple(0, 10, 1000, 2000);
        aggerator.on_local_feedback(1, FeatureControlActor::Controller, fb);
        assert_eq!(aggerator.pop_output(), Some(fb));
        assert_eq!(aggerator.pop_output(), None);

        let fb = Feedback::simple(0, 20, 1500, 3000);
        aggerator.on_local_feedback(2, FeatureControlActor::Worker(0), fb);

        aggerator.on_tick(1000);
        assert_eq!(
            aggerator.pop_output(),
            Some(Feedback {
                kind: 0,
                count: 2,
                max: 20,
                min: 10,
                sum: 30,
                interval_ms: 1000,
                timeout_ms: 3000,
            })
        );
        assert_eq!(aggerator.pop_output(), None);
    }

    #[test]
    fn aggerator_multi_types() {
        let mut aggerator = FeedbacksAggerator::default();
        let fb1 = Feedback::simple(0, 10, 1000, 2000);
        aggerator.on_local_feedback(1, FeatureControlActor::Controller, fb1);

        let fb2 = Feedback::simple(1, 20, 1500, 3000);
        aggerator.on_local_feedback(2, FeatureControlActor::Controller, fb2);

        aggerator.on_tick(3);
        assert_eq!(aggerator.pop_output(), Some(fb1));
        assert_eq!(aggerator.pop_output(), Some(fb2));
        assert_eq!(aggerator.pop_output(), None);
    }

    #[test]
    fn aggerator_auto_clear_kind_nodata() {
        let mut aggerator = FeedbacksAggerator::default();
        let fb1 = Feedback::simple(0, 10, 1000, 2000);
        aggerator.on_local_feedback(0, FeatureControlActor::Controller, fb1);

        assert_eq!(aggerator.feedbacks.len(), 1);

        aggerator.on_tick(2000);
        assert_eq!(aggerator.feedbacks.len(), 0);
    }
}

use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    sync::Arc,
};

use bluesea_identity::NodeId;
use utils::awaker::Awaker;

use super::{ChannelUuid, LocalSubId};

struct ChannelContainer {
    sources: Vec<NodeId>,
    subs: Vec<LocalSubId>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SourceBindingAction {
    Subscribe(ChannelUuid),
    Unsubscribe(ChannelUuid),
}

pub struct SourceBinding {
    channels: HashMap<ChannelUuid, ChannelContainer>,
    actions: VecDeque<SourceBindingAction>,
    awaker: Arc<dyn Awaker>,
}

impl SourceBinding {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
            actions: VecDeque::new(),
            awaker: Arc::new(utils::awaker::MockAwaker::default()),
        }
    }

    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.awaker = awaker;
    }

    pub fn on_local_sub(&mut self, channel: ChannelUuid, sub: LocalSubId) -> Option<Vec<NodeId>> {
        match self.channels.entry(channel) {
            Entry::Occupied(mut entry) => {
                // only push to subs when not exist
                if !entry.get().subs.contains(&sub) {
                    if entry.get().subs.is_empty() {
                        self.actions.push_back(SourceBindingAction::Subscribe(channel));
                        self.awaker.notify();
                    }
                    entry.get_mut().subs.push(sub);
                    if entry.get().sources.is_empty() {
                        None
                    } else {
                        Some(entry.get().sources.clone())
                    }
                } else {
                    None
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(ChannelContainer { sources: vec![], subs: vec![sub] });
                self.actions.push_back(SourceBindingAction::Subscribe(channel));
                self.awaker.notify();
                None
            }
        }
    }

    pub fn on_local_unsub(&mut self, channel: ChannelUuid, sub: LocalSubId) -> Option<Vec<NodeId>> {
        let container = self.channels.get_mut(&channel)?;
        let index = container.subs.iter().position(|x| *x == sub)?;
        container.subs.remove(index);

        if container.subs.is_empty() {
            self.actions.push_back(SourceBindingAction::Unsubscribe(channel));
            self.awaker.notify();
        }

        if container.subs.is_empty() && container.sources.is_empty() {
            self.channels.remove(&channel);
            None
        } else {
            if container.sources.is_empty() {
                None
            } else {
                Some(container.sources.clone())
            }
        }
    }

    pub fn on_source_added(&mut self, channel: ChannelUuid, source: NodeId) -> Option<Vec<LocalSubId>> {
        match self.channels.entry(channel) {
            Entry::Occupied(mut entry) => {
                // only push to sources when not exist
                if !entry.get().sources.contains(&source) {
                    entry.get_mut().sources.push(source);
                    if entry.get().subs.is_empty() {
                        None
                    } else {
                        Some(entry.get().subs.clone())
                    }
                } else {
                    None
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(ChannelContainer { sources: vec![source], subs: vec![] });
                None
            }
        }
    }

    pub fn on_source_removed(&mut self, channel: ChannelUuid, source: NodeId) -> Option<Vec<LocalSubId>> {
        let container = self.channels.get_mut(&channel)?;
        let index = container.sources.iter().position(|x| *x == source)?;
        container.sources.remove(index);

        if container.subs.is_empty() && container.sources.is_empty() {
            self.channels.remove(&channel);
            None
        } else {
            if container.subs.is_empty() {
                None
            } else {
                Some(container.subs.clone())
            }
        }
    }

    pub fn sources_for(&self, channel: ChannelUuid) -> Vec<NodeId> {
        self.channels.get(&channel).map(|x| x.sources.clone()).unwrap_or_default()
    }

    pub fn pop_action(&mut self) -> Option<SourceBindingAction> {
        self.actions.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use utils::awaker::Awaker;

    use crate::relay::source_binding::SourceBindingAction;

    use super::SourceBinding;

    #[test]
    fn source_for_should_correct() {
        let mut bindding = SourceBinding::new();
        assert_eq!(bindding.sources_for(1), vec![]);

        bindding.on_source_added(1, 1000);
        bindding.on_source_added(1, 1001);

        assert_eq!(bindding.sources_for(1), vec![1000, 1001]);
    }

    #[test]
    fn local_sub_unsub_should_correct() {
        let awake = Arc::new(utils::awaker::MockAwaker::default());
        let mut bindding = SourceBinding::new();
        bindding.set_awaker(awake.clone());

        assert_eq!(bindding.on_source_added(1, 1000), None);
        assert_eq!(bindding.on_source_added(1, 1001), None);

        assert_eq!(bindding.on_local_unsub(1, 10), None);
        assert_eq!(bindding.on_local_unsub(1, 11), None);

        assert_eq!(bindding.on_local_sub(1, 10), Some(vec![1000, 1001]));
        assert_eq!(bindding.pop_action(), Some(SourceBindingAction::Subscribe(1)));
        assert_eq!(bindding.pop_action(), None);
        assert_eq!(awake.pop_awake_count(), 1);

        assert_eq!(bindding.on_local_sub(1, 10), None); // already sub
        assert_eq!(bindding.on_local_unsub(1, 11), None);

        assert_eq!(bindding.on_local_sub(1, 11), Some(vec![1000, 1001]));
        assert_eq!(bindding.pop_action(), None);
        assert_eq!(awake.pop_awake_count(), 0);

        assert_eq!(bindding.on_local_unsub(1, 10), Some(vec![1000, 1001]));
        assert_eq!(bindding.pop_action(), None);
        assert_eq!(awake.pop_awake_count(), 0);

        assert_eq!(bindding.on_local_unsub(1, 11), Some(vec![1000, 1001]));
        assert_eq!(bindding.pop_action(), Some(SourceBindingAction::Unsubscribe(1)));
        assert_eq!(bindding.pop_action(), None);
        assert_eq!(awake.pop_awake_count(), 1);

        assert_eq!(bindding.on_local_unsub(1, 10), None);
        assert_eq!(bindding.on_local_unsub(1, 11), None);
    }

    #[test]
    fn source_add_remove_should_correct() {
        let mut bindding = SourceBinding::new();

        assert_eq!(bindding.on_local_sub(1, 10), None);
        assert_eq!(bindding.on_local_sub(1, 11), None);

        assert_eq!(bindding.on_source_removed(1, 1000), None);
        assert_eq!(bindding.on_source_removed(1, 1001), None);

        assert_eq!(bindding.on_source_added(1, 1000), Some(vec![10, 11]));
        assert_eq!(bindding.on_source_added(1, 1001), Some(vec![10, 11]));

        assert_eq!(bindding.on_source_added(1, 1000), None); // already added
        assert_eq!(bindding.on_source_added(1, 1001), None); // already added

        assert_eq!(bindding.on_source_removed(1, 1000), Some(vec![10, 11]));
        assert_eq!(bindding.on_source_removed(1, 1001), Some(vec![10, 11]));

        assert_eq!(bindding.on_source_removed(1, 1000), None); // already removed
        assert_eq!(bindding.on_source_removed(1, 1001), None); // already removed
    }
}

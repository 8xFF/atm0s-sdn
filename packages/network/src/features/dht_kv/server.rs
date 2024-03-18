use std::collections::{HashMap, VecDeque};

use self::map::RemoteMap;

use super::{
    msg::{ClientCommand, NodeSession, ServerEvent, ServerMapEvent},
    seq::SeqGenerator,
    Key,
};

mod map;

pub struct RemoteStorage {
    maps: HashMap<Key, RemoteMap>,
    seq_gen: SeqGenerator,
    queue: VecDeque<(NodeSession, ServerEvent)>,
}

impl RemoteStorage {
    pub fn new() -> Self {
        Self {
            maps: HashMap::new(),
            seq_gen: SeqGenerator::default(),
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        // let mut to_remove = vec![];
        // for (key, map) in self.maps.iter_mut() {
        //     if let Some(syncs) = map.on_tick(now) {
        //         for cmd in syncs {
        //             self.queue.push_back((NodeId::default(), ServerCommand::Map(*key, cmd)));
        //         }
        //     }
        //     if map.should_clean() {
        //         to_remove.push(*key);
        //     }
        // }

        // for key in to_remove {
        //     self.maps.remove(&key);
        // }
    }

    pub fn on_remote(&mut self, now: u64, remote: NodeSession, cmd: ClientCommand) {
        match cmd {
            ClientCommand::MapCmd(key, cmd) => {
                let map = if let Some(map) = self.maps.get_mut(&key) {
                    map
                } else {
                    if cmd.is_creator() {
                        self.maps.insert(key, RemoteMap::default());
                        self.maps.get_mut(&key).expect("Must have value with previous insterted")
                    } else {
                        return;
                    }
                };

                if let Some(event) = map.on_client(now, remote, cmd) {
                    self.queue.push_back((remote, ServerEvent::Map(key, event)));
                }
            }
            ClientCommand::MapGet(key, id) => {
                let values = self.maps.get_mut(&key).map(|map| map.dump()).unwrap_or_default();
                self.queue.push_back((remote, ServerEvent::Map(key, ServerMapEvent::GetOk(id, values))));
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<(NodeSession, ServerEvent)> {
        self.queue.pop_front()
    }
}

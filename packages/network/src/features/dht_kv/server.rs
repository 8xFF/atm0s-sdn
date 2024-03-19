use std::collections::{HashMap, VecDeque};

use self::map::RemoteMap;

use super::{
    msg::{ClientCommand, NodeSession, ServerEvent},
    Map,
};

mod map;

pub struct RemoteStorage {
    session: NodeSession,
    maps: HashMap<Map, RemoteMap>,
    queue: VecDeque<(NodeSession, ServerEvent)>,
}

impl RemoteStorage {
    pub fn new(session: NodeSession) -> Self {
        Self {
            session,
            maps: HashMap::new(),
            queue: VecDeque::new(),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        let mut to_remove = vec![];
        for (key, map) in self.maps.iter_mut() {
            map.on_tick(now);
            while let Some((session, event)) = map.pop_action() {
                self.queue.push_back((session, ServerEvent::MapEvent(*key, event)));
            }
            if map.should_clean() {
                to_remove.push(*key);
            }
        }

        for key in to_remove {
            self.maps.remove(&key);
        }
    }

    pub fn on_remote(&mut self, now: u64, remote: NodeSession, cmd: ClientCommand) {
        match cmd {
            ClientCommand::MapCmd(key, cmd) => {
                let map = if let Some(map) = self.maps.get_mut(&key) {
                    map
                } else {
                    if cmd.is_creator() {
                        self.maps.insert(key, RemoteMap::new(self.session));
                        self.maps.get_mut(&key).expect("Must have value with previous insterted")
                    } else {
                        return;
                    }
                };

                if let Some(event) = map.on_client(now, remote, cmd) {
                    self.queue.push_back((remote, ServerEvent::MapEvent(key, event)));
                    while let Some((session, event)) = map.pop_action() {
                        self.queue.push_back((session, ServerEvent::MapEvent(key, event)));
                    }
                }
            }
            ClientCommand::MapGet(key, id) => {
                let values = self.maps.get_mut(&key).map(|map| map.dump()).unwrap_or_default();
                self.queue.push_back((remote, ServerEvent::MapGetRes(key, id, values)));
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<(NodeSession, ServerEvent)> {
        self.queue.pop_front()
    }
}

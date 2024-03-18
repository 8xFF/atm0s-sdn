use atm0s_sdn_router::RouteRule;
use std::collections::{HashMap, VecDeque};

use crate::base::ServiceId;

use self::map::{LocalMap, LocalMapOutput};

use super::{
    msg::{ClientCommand, NodeSession, ServerEvent},
    Control, Event, Key,
};

mod map;

fn route(key: Key) -> RouteRule {
    RouteRule::ToKey(key.0 as u32)
}

pub enum LocalStorageOutput {
    Local(ServiceId, Event),
    Remote(RouteRule, ClientCommand),
}

pub struct LocalStorage {
    session: NodeSession,
    maps: HashMap<Key, LocalMap>,
    queue: VecDeque<LocalStorageOutput>,
}

impl LocalStorage {
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
            while let Some(out) = map.pop_action() {
                self.queue.push_back(match out {
                    LocalMapOutput::Local(service, event) => LocalStorageOutput::Local(service, Event::Map(*key, event)),
                    LocalMapOutput::Remote(cmd) => LocalStorageOutput::Remote(route(*key), ClientCommand::MapCmd(*key, cmd)),
                });
            }
            if map.should_cleanup() {
                to_remove.push(*key);
            }
        }

        for key in to_remove {
            self.maps.remove(&key);
        }
    }

    pub fn on_local(&mut self, now: u64, service: ServiceId, control: Control) {
        match control {
            Control::Map(key, control) => {
                if let Some(map) = Self::get_map(&mut self.maps, self.session, key, control.is_creator()) {
                    if let Some(event) = map.on_control(now, service, control) {
                        self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::MapCmd(key, event)));
                        while let Some(out) = map.pop_action() {
                            self.queue.push_back(match out {
                                LocalMapOutput::Local(service, event) => LocalStorageOutput::Local(service, Event::Map(key, event)),
                                LocalMapOutput::Remote(cmd) => LocalStorageOutput::Remote(route(key), ClientCommand::MapCmd(key, cmd)),
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn on_server(&mut self, now: u64, remote: NodeSession, cmd: ServerEvent) {
        match cmd {
            ServerEvent::Map(key, cmd) => {
                if let Some(map) = self.maps.get_mut(&key) {
                    if let Some(cmd) = map.on_server(now, remote, cmd) {
                        self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::MapCmd(key, cmd)));
                    }
                } else {
                    log::warn!("Received remote command for unknown map: {:?}", key);
                }
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<LocalStorageOutput> {
        self.queue.pop_front()
    }

    fn get_map(maps: &mut HashMap<Key, LocalMap>, session: NodeSession, key: Key, auto_create: bool) -> Option<&mut LocalMap> {
        if !maps.contains_key(&key) && auto_create {
            maps.insert(key, LocalMap::new(session));
        }
        maps.get_mut(&key)
    }
}

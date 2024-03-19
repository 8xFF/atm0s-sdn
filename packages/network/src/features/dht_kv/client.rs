use atm0s_sdn_router::RouteRule;
use std::collections::{HashMap, VecDeque};

use crate::base::FeatureControlActor;

use self::map::{LocalMap, LocalMapOutput};

const MAP_GET_TIMEOUT_MS: u64 = 5000;

use super::{
    msg::{ClientCommand, NodeSession, ServerEvent},
    Control, Event, Map,
};

mod map;

fn route(key: Map) -> RouteRule {
    RouteRule::ToKey(key.0 as u32)
}

pub enum LocalStorageOutput {
    Local(FeatureControlActor, Event),
    Remote(RouteRule, ClientCommand),
}

pub struct LocalStorage {
    session: NodeSession,
    maps: HashMap<Map, LocalMap>,
    map_get_waits: HashMap<(Map, u64), (FeatureControlActor, u64)>,
    queue: VecDeque<LocalStorageOutput>,
    req_id_seed: u64,
}

impl LocalStorage {
    pub fn new(session: NodeSession) -> Self {
        Self {
            session,
            maps: HashMap::new(),
            map_get_waits: HashMap::new(),
            queue: VecDeque::new(),
            req_id_seed: 0,
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        // tick all maps and finding out if any of them should be removed
        let mut to_remove = vec![];
        for (key, map) in self.maps.iter_mut() {
            map.on_tick(now);
            Self::pop_map_actions(*key, map, &mut self.queue);
            if map.should_cleanup() {
                to_remove.push(*key);
            }
        }

        for key in to_remove {
            self.maps.remove(&key);
        }

        // finding timeout map_get requests
        let mut to_remove = vec![];
        for (key, info) in self.map_get_waits.iter() {
            if now >= info.1 + MAP_GET_TIMEOUT_MS {
                to_remove.push(*key);
            }
        }

        for key in to_remove {
            self.map_get_waits.remove(&key);
        }
    }

    pub fn on_local(&mut self, now: u64, actor: FeatureControlActor, control: Control) {
        match control {
            Control::MapCmd(key, control) => {
                if let Some(map) = Self::get_map(&mut self.maps, self.session, key, control.is_creator()) {
                    if let Some(event) = map.on_control(now, actor, control) {
                        self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::MapCmd(key, event)));
                    }
                    Self::pop_map_actions(key, map, &mut self.queue);
                }
            }
            Control::MapGet(key) => {
                let req_id = self.req_id_seed;
                self.req_id_seed += 1;
                self.map_get_waits.insert((key, req_id), (actor, req_id));
                self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::MapGet(key, req_id)));
            }
        }
    }

    pub fn on_server(&mut self, now: u64, remote: NodeSession, cmd: ServerEvent) {
        match cmd {
            ServerEvent::MapEvent(key, cmd) => {
                if let Some(map) = self.maps.get_mut(&key) {
                    if let Some(cmd) = map.on_server(now, remote, cmd) {
                        self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::MapCmd(key, cmd)));
                    }
                    Self::pop_map_actions(key, map, &mut self.queue);
                } else {
                    log::warn!("Received remote command for unknown map: {:?}", key);
                }
            }
            ServerEvent::MapGetRes(key, req_id, res) => {
                if let Some((actor, _time_ms)) = self.map_get_waits.remove(&(key, req_id)) {
                    self.queue.push_back(LocalStorageOutput::Local(actor, Event::MapGetRes(key, Ok(res))));
                }
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<LocalStorageOutput> {
        self.queue.pop_front()
    }

    fn get_map(maps: &mut HashMap<Map, LocalMap>, session: NodeSession, key: Map, auto_create: bool) -> Option<&mut LocalMap> {
        if !maps.contains_key(&key) && auto_create {
            log::info!("[DhtKvClient] Creating new map: {}", key);
            maps.insert(key, LocalMap::new(session));
        }
        maps.get_mut(&key)
    }

    fn pop_map_actions(key: Map, map: &mut LocalMap, queue: &mut VecDeque<LocalStorageOutput>) {
        while let Some(out) = map.pop_action() {
            queue.push_back(match out {
                LocalMapOutput::Local(actor, event) => LocalStorageOutput::Local(actor, Event::MapEvent(key, event)),
                LocalMapOutput::Remote(cmd) => LocalStorageOutput::Remote(route(key), ClientCommand::MapCmd(key, cmd)),
            });
        }
    }
}

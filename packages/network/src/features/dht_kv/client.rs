use atm0s_sdn_router::RouteRule;
use std::collections::{HashMap, VecDeque};

use crate::base::ServiceId;

// use self::map::LocalMap;

use super::{
    msg::{ClientCommand, NodeSession, ServerEvent},
    seq::SeqGenerator,
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
    // maps: HashMap<Key, LocalMap>,
    queue: VecDeque<LocalStorageOutput>,
    seq_gen: SeqGenerator,
}

impl LocalStorage {
    pub fn new() -> Self {
        Self {
            // maps: HashMap::new(),
            queue: VecDeque::new(),
            seq_gen: SeqGenerator::default(),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        // let mut to_remove = Vec::new();
        // for (key, map) in self.maps.iter_mut() {
        //     if let Some(syncs) = map.on_tick(now) {
        //         for cmd in syncs {
        //             self.queue.push_back(LocalStorageOutput::Remote(route(*key), ClientCommand::Map(*key, cmd)));
        //         }
        //     }

        //     if map.should_cleanup() {
        //         to_remove.push(*key);
        //     }
        // }

        // for key in to_remove {
        //     self.maps.remove(&key);
        // }
    }

    pub fn on_local(&mut self, now: u64, service: ServiceId, control: Control) {
        // match control {
        //     Control::HSet(key, sub, value) => {
        //         let map = self.maps.entry(key).or_insert_with(|| LocalMap::default());
        //         if let Some(cmd) = map.set_local(now, sub, value, self.seq_gen.next()) {
        //             self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::Map(key, cmd)));
        //         }
        //     },
        //     Control::HGet(key, sub) => {
        //         todo!()
        //     },
        //     Control::HDel(key, sub) => {
        //         if let Some(map) = self.maps.get_mut(&key) {
        //             if let Some(cmd) = map.del_local(now, sub, self.seq_gen.next()) {
        //                 self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::Map(key, cmd)));
        //             }
        //         }
        //     },
        //     Control::HSub(key) => {
        //         let map = self.maps.entry(key).or_insert_with(|| LocalMap::default());
        //         if let Some(cmd) = map.sub_local(now, service, self.seq_gen.next()) {
        //             self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::Map(key, cmd)));
        //         }
        //     },
        //     Control::HUnsub(key) => {
        //         if let Some(map) = self.maps.get_mut(&key) {
        //             if let Some(cmd) = map.unsub_local(now, service, self.seq_gen.next()) {
        //                 self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::Map(key, cmd)));
        //             }
        //         }
        //     },
        // }
    }

    pub fn on_server(&mut self, now: u64, remote: NodeSession, cmd: ServerEvent) {
        // match cmd {
        //     ServerEvent::Map(key, cmd) => {
        //         if let Some(map) = self.maps.get_mut(&key) {
        //             if let Some(cmd) = map.on_server(now, remote, cmd) {
        //                 self.queue.push_back(LocalStorageOutput::Remote(route(key), ClientCommand::Map(key, cmd)));
        //             }
        //         } else {
        //             log::warn!("Received remote command for unknown map: {:?}", key);
        //         }
        //     },
        // }
    }

    pub fn pop_action(&mut self) -> Option<LocalStorageOutput> {
        self.queue.pop_front()
    }
}

// use std::collections::HashMap;

// use atm0s_sdn_identity::NodeId;

// use crate::{base::ServiceId, features::dht_kv::{msg::NodeSession, SubKey}};

// const RESEND_MS: u64 = 200; //We will resend set or del command if we don't get ack in this time
// struct Locker {
//     source: NodeId,
//     source_session: Session,
// }

// enum MapSlot {
//     Unspecific,
//     Remote {
//         value: Option<Vec<u8>>,
//         locker: Locker,
//     },
//     Local {
//         value: Option<Vec<u8>>,
//         syncing: Option<Seq>,
//     },
// }

// impl MapSlot {
//     fn set_local(&mut self, value: Vec<u8>, seq: Seq) -> Option<ClientValueCommand> {
//         todo!()
//     }

//     fn del_local(&mut self, seq: Seq) -> Option<ClientValueCommand> {
//         todo!()
//     }

//     fn on_server(&mut self, remote: NodeId, remote_session: Session, cmd: ServerValueCommand) -> Option<ClientValueCommand> {
//         todo!()
//     }
// }

// #[derive(Default)]
// pub struct LocalMap {
//     values: HashMap<SubKey, MapSlot>,
//     subscribers: Vec<ServiceId>,
//     sub_syncing: Option<Seq>,
// }

// impl LocalMap {
//     pub fn on_tick(&mut self, now: u64) -> Option<Vec<MapClientCommand>> {
//         todo!()
//     }

//     pub fn set_local(&mut self, now: u64, sub: SubKey, value: Vec<u8>, seq: Seq) -> Option<MapClientCommand> {
//         todo!()
//     }

//     pub fn del_local(&mut self, now: u64, sub: SubKey, seq: Seq) -> Option<MapClientCommand> {
//         todo!()
//     }

//     pub fn sub_local(&mut self, now: u64, service: ServiceId, seq: Seq) -> Option<MapClientCommand> {
//         todo!()
//     }

//     pub fn unsub_local(&mut self, now: u64, service: ServiceId, seq: Seq) -> Option<MapClientCommand> {
//         todo!()
//     }

//     pub fn on_server(&mut self, now: u64, remote: NodeSession, cmd: MapServerCommand) -> Option<MapClientCommand> {
//         todo!()
//     }

//     pub fn should_cleanup(&self) -> bool {
//         todo!()
//     }
// }

use self::table::ShadowTable;

mod table;

#[derive(Debug, Clone)]
pub enum ShadowRouterDelta<Remote> {
    SetTable { layer: u8, index: u8, remote: Remote },
    DelTable { layer: u8, index: u8 },
    SetServiceRemote { service: u8, remote: Remote },
    DelServiceRemote { service: u8 },
    SetServiceLocal { service: u8 },
    DelServiceLocal { service: u8 },
}

pub struct ShadowRouter<Remote> {
    local_registries: [bool; 256],
    remote_registry: [Option<Remote>; 256],
    tables: [ShadowTable<Remote>; 4],
}

impl<Remote: Copy> ShadowRouter<Remote> {
    pub fn new() -> Self {
        Self {
            local_registries: [false; 256],
            remote_registry: [None; 256],
            tables: [ShadowTable::new(), ShadowTable::new(), ShadowTable::new(), ShadowTable::new()],
        }
    }

    pub fn apply_delta(&mut self, delta: ShadowRouterDelta<Remote>) {
        match delta {
            ShadowRouterDelta::SetTable { layer, index, remote } => {
                self.tables[layer as usize].set(index, remote);
            }
            ShadowRouterDelta::DelTable { layer, index } => {
                self.tables[layer as usize].del(index);
            }
            ShadowRouterDelta::SetServiceRemote { service, remote } => {
                self.remote_registry[service as usize] = Some(remote);
            }
            ShadowRouterDelta::DelServiceRemote { service } => {
                self.remote_registry[service as usize] = None;
            }
            ShadowRouterDelta::SetServiceLocal { service } => {
                self.local_registries[service as usize] = true;
            }
            ShadowRouterDelta::DelServiceLocal { service } => {
                self.local_registries[service as usize] = false;
            }
        }
    }
}

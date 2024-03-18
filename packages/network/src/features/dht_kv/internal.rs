use atm0s_sdn_router::RouteRule;

use crate::base::ServiceId;

use super::{
    client::{LocalStorage, LocalStorageOutput},
    msg::{NodeSession, RemoteCommand},
    server::RemoteStorage,
    Control, Event,
};

pub enum InternalOutput {
    Local(ServiceId, Event),
    Remote(RouteRule, RemoteCommand),
}

pub struct DhtKvInternal {
    session: NodeSession,
    local: LocalStorage,
    remote: RemoteStorage,
}

impl DhtKvInternal {
    pub fn new(session: NodeSession) -> Self {
        Self {
            session,
            local: LocalStorage::new(),
            remote: RemoteStorage::new(),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        self.local.on_tick(now);
        self.remote.on_tick(now);
    }

    pub fn on_local(&mut self, now: u64, service: ServiceId, control: Control) {
        self.local.on_local(now, service, control);
    }

    pub fn on_remote(&mut self, now: u64, cmd: RemoteCommand) {
        match cmd {
            RemoteCommand::Client(remote, cmd) => self.remote.on_remote(now, remote, cmd),
            RemoteCommand::Server(remote, cmd) => {
                self.local.on_server(now, remote, cmd);
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<InternalOutput> {
        if let Some(out) = self.local.pop_action() {
            match out {
                LocalStorageOutput::Remote(rule, cmd) => Some(InternalOutput::Remote(rule, RemoteCommand::Client(self.session, cmd))),
                LocalStorageOutput::Local(service, event) => Some(InternalOutput::Local(service, event)),
            }
        } else if let Some((session, cmd)) = self.remote.pop_action() {
            Some(InternalOutput::Remote(RouteRule::ToNode(session.0), RemoteCommand::Server(self.session, cmd)))
        } else {
            None
        }
    }
}

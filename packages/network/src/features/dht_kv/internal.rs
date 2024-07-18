use std::fmt::Debug;

use atm0s_sdn_router::RouteRule;

use crate::base::FeatureControlActor;

use super::{
    client::{LocalStorage, LocalStorageOutput},
    msg::{NodeSession, RemoteCommand},
    server::RemoteStorage,
    Control, Event,
};

pub enum InternalOutput<UserData> {
    Local(FeatureControlActor<UserData>, Event),
    Remote(RouteRule, RemoteCommand),
}

pub struct DhtKvInternal<UserData> {
    session: NodeSession,
    local: LocalStorage<UserData>,
    remote: RemoteStorage,
}

impl<UserData: Eq + Debug + Copy> DhtKvInternal<UserData> {
    pub fn new(session: NodeSession) -> Self {
        Self {
            session,
            local: LocalStorage::new(session),
            remote: RemoteStorage::new(session),
        }
    }

    pub fn on_tick(&mut self, now: u64) {
        self.local.on_tick(now);
        self.remote.on_tick(now);
    }

    pub fn on_local(&mut self, now: u64, actor: FeatureControlActor<UserData>, control: Control) {
        self.local.on_local(now, actor, control);
    }

    pub fn on_remote(&mut self, now: u64, cmd: RemoteCommand) {
        log::debug!("[DhtKvInternal] on_remote: {:?}", cmd);
        match cmd {
            RemoteCommand::Client(remote, cmd) => self.remote.on_remote(now, remote, cmd),
            RemoteCommand::Server(remote, cmd) => {
                self.local.on_server(now, remote, cmd);
            }
        }
    }

    pub fn pop_action(&mut self) -> Option<InternalOutput<UserData>> {
        if let Some(out) = self.local.pop_action() {
            match out {
                LocalStorageOutput::Remote(rule, cmd) => {
                    log::debug!("[DhtKvInternal] Sending to {:?} cmd {:?}", rule, cmd);
                    Some(InternalOutput::Remote(rule, RemoteCommand::Client(self.session, cmd)))
                }
                LocalStorageOutput::Local(actor, event) => {
                    log::debug!("[DhtKvInternal] Send to actor {:?} event: {:?}", actor, event);
                    Some(InternalOutput::Local(actor, event))
                }
            }
        } else if let Some((session, cmd)) = self.remote.pop_action() {
            log::debug!("[DhtKvInternal] Sending to node {} cmd {:?}", session.0, cmd);
            Some(InternalOutput::Remote(RouteRule::ToNode(session.0), RemoteCommand::Server(self.session, cmd)))
        } else {
            None
        }
    }
}

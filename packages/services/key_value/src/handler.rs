use bluesea_identity::{ConnId, NodeId};
use network::behaviour::ConnectionHandler;
use network::transport::ConnectionEvent;
use network::ConnectionAgent;
use router::SharedRouter;

pub struct KeyValueConnectionHandler {
    router: SharedRouter,
}

impl KeyValueConnectionHandler {
    pub fn new(router: SharedRouter) -> Self {
        Self { router }
    }
}

impl<BE, HE, Msg> ConnectionHandler<BE, HE, Msg> for KeyValueConnectionHandler {
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, Msg>) {
        todo!()
    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, ts_ms: u64, interal_ms: u64) {
        todo!()
    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, event: ConnectionEvent<Msg>) {
        todo!()
    }

    fn on_other_handler_event(
        &mut self,
        agent: &ConnectionAgent<BE, HE, Msg>,
        from_node: NodeId,
        from_conn: ConnId,
        event: HE,
    ) {
        todo!()
    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, event: HE) {
        todo!()
    }

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, Msg>) {
        todo!()
    }
}

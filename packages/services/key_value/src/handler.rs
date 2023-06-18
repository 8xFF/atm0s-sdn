use bluesea_identity::{ConnId, NodeId};
use network::behaviour::ConnectionHandler;
use network::transport::{ConnectionEvent, ConnectionMsg};
use network::{ConnectionAgent, CrossHandlerRoute};
use router::SharedRouter;
use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};

pub struct KeyValueConnectionHandler {
    router: SharedRouter,
}

impl KeyValueConnectionHandler {
    pub fn new(router: SharedRouter) -> Self {
        Self { router }
    }
}

impl<BE, HE, Msg> ConnectionHandler<BE, HE, Msg> for KeyValueConnectionHandler
    where HE: Send + Sync + 'static,
          BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
          Msg: From<KeyValueMsg> + TryInto<KeyValueMsg> + Send + Sync + 'static,
{
    fn on_opened(&mut self, agent: &ConnectionAgent<BE, HE, Msg>) {

    }

    fn on_tick(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, ts_ms: u64, interal_ms: u64) {

    }

    fn on_event(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, event: ConnectionEvent<Msg>) {
        match event {
            ConnectionEvent::Msg { msg, service_id } => match msg {
                ConnectionMsg::Reliable { data, stream_id } => {
                    if let Ok(kv_msg) = data.try_into() {
                        let forward_to = match &kv_msg {
                            KeyValueMsg::KeyValueServer(_, routing_to, _, _) => {
                                self.router.closest_node(*routing_to, &vec![])
                                    .map(|(conn, node, _, _)| (conn, node))
                            }
                            KeyValueMsg::KeyValueClient(_, routing_to, _, _) => {
                                self.router.closest_node(*routing_to, &vec![])
                                    .map(|(conn, node, _, _)| (conn, node))
                            }
                            KeyValueMsg::Ack(_, dest) => {
                                self.router.next(*dest, &vec![])
                            }
                        };

                        if let Some((next_conn, next_node)) = forward_to {
                            agent.send_to_net(
                                CrossHandlerRoute::Conn(next_conn),
                                ConnectionMsg::Reliable {
                                    data: kv_msg.into(),
                                    stream_id,
                                },
                            );
                        } else {
                            agent.send_behavior(KeyValueBehaviorEvent::FromNode(kv_msg).into());
                        }
                    }
                }
                ConnectionMsg::Unreliable { .. } => {}
            }
            ConnectionEvent::Stats(_) => {}
        }
    }

    fn on_other_handler_event(
        &mut self,
        agent: &ConnectionAgent<BE, HE, Msg>,
        from_node: NodeId,
        from_conn: ConnId,
        event: HE,
    ) {

    }

    fn on_behavior_event(&mut self, agent: &ConnectionAgent<BE, HE, Msg>, event: HE) {

    }

    fn on_closed(&mut self, agent: &ConnectionAgent<BE, HE, Msg>) {

    }
}

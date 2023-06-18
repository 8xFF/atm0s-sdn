use crate::handler::KeyValueConnectionHandler;
use crate::logic::key_value::client::KeyValueClient;
use crate::logic::key_value::server::KeyValueServer;
use crate::logic::key_value::KeyValueServerAction;
use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg, StorageAction, StorageActionRetryStrategy, StorageActionRouting, SubAction};
use crate::KEY_VALUE_SERVICE_ID;
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::transport::{ConnectionMsg, ConnectionRejectReason, ConnectionSender, MsgRoute, OutgoingConnectionError, RpcAnswer};
use network::{BehaviorAgent, CrossHandlerRoute};
use router::SharedRouter;
use std::sync::Arc;
use utils::hashmap::HashMap;
use utils::random::Random;
use utils::Timer;

struct ActionSlot {
    count: u32,
    action: StorageAction,
}

pub struct KeyValueBehavior {
    router: SharedRouter,
    key_value_server: KeyValueServer,
    key_value_client: KeyValueClient,
    wait_actions: HashMap<u64, ActionSlot>,
}

impl KeyValueBehavior {
    pub fn new(router: SharedRouter, random: Arc<dyn Random<u64>>, timer: Arc<dyn Timer>) -> Self {
        let node_id = router.node_id();
        Self {
            router: router.clone(),
            key_value_server: KeyValueServer::new(router, random.clone(), timer.clone()),
            key_value_client: KeyValueClient::new(node_id, random.clone(), timer.clone()),
            wait_actions: Default::default(),
        }
    }

    fn process_key_value_msg<HE, Msg>(router: &SharedRouter, msg: KeyValueMsg, server: &mut KeyValueServer, wait_actions: &mut HashMap<u64, ActionSlot>, agent: &BehaviorAgent<HE, Msg>)
    where
        HE: Send + Sync + 'static,
        Msg: From<KeyValueMsg> + TryInto<KeyValueMsg> + Send + Sync + 'static,
    {
        let ack_dest = match msg {
            KeyValueMsg::KeyValueServer(action_id, _routing_to, sender, action) => {
                server.on_remote(action);
                Some((action_id, sender))
            }
            KeyValueMsg::KeyValueClient(action_id, _routing_to, sender, _event) => {
                //TODO sending to handler logics
                Some((action_id, sender))
            }
            KeyValueMsg::Ack(action_id, _destination) => {
                wait_actions.remove(&action_id);
                None
            }
        };

        if let Some((action_id, destination)) = ack_dest {
            agent.send_to_net(
                MsgRoute::Node(destination),
                10,
                ConnectionMsg::Reliable {
                    stream_id: 0,
                    data: KeyValueMsg::Ack(action_id, destination).into(),
                },
            );
        }
    }
}

impl<BE, HE, Msg, Req, Res> NetworkBehavior<BE, HE, Msg, Req, Res> for KeyValueBehavior
where
    BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
    HE: Send + Sync + 'static,
    Msg: From<KeyValueMsg> + TryInto<KeyValueMsg> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        KEY_VALUE_SERVICE_ID
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<HE, Msg>, ts_ms: u64, interal_ms: u64) {
        // self.key_value_server.tick();
        // self.key_value_client.tick();
        // while let Some(action) = self.key_value_server.poll() {
        //     self.wait_actions.insert(action.action_id, ActionSlot {
        //         count: 0,
        //         action,
        //     });
        // }
        //
        // while let Some(action) = self.key_value_client.poll() {
        //     self.wait_actions.insert(action.action_id, ActionSlot {
        //         count: 0,
        //         action,
        //     });
        // }
        //
        // let mut remove_action_ids = vec![];
        // for (action_id, slot) in self.wait_actions.iter() {
        //     let next = match slot.action.routing {
        //         StorageActionRouting::Node(dest_node) => {
        //             self.router.next(dest_node, &vec![])
        //                 .map(|(next_conn, _)| next_conn)
        //         }
        //         StorageActionRouting::ClosestNode(routing_key) => {
        //             self.router.closest_node(routing_key as u32, &vec![])
        //                 .map(|(next_conn, _, _, _)| next_conn)
        //         }
        //     };
        //
        //     if let Some(next_conn) = next {
        //         agent.send_to_net(CrossHandlerRoute::Conn(next_conn), ConnectionMsg::Reliable {
        //             stream_id: 0,
        //             data: match &slot.action.sub_action {
        //                 SubAction::KeyValueServer(action) => {
        //                     KeyValueMsg::KeyValueServer(
        //                         slot.action.action_id,
        //                         agent.local_node_id(),
        //                         slot.action.routing.routing_key(),
        //                         action.clone()
        //                     ).into()
        //                 }
        //                 SubAction::KeyValueClient(event) => {
        //                     KeyValueMsg::KeyValueClient(
        //                         slot.action.action_id,
        //                         agent.local_node_id(),
        //                         slot.action.routing.routing_key(),
        //                         event.clone()
        //                     ).into()
        //                 }
        //             }
        //         });
        //         // slot.count += 1;
        //         let should_remove = match slot.action.retry {
        //             StorageActionRetryStrategy::Retry(limit) => limit <= slot.count,
        //             StorageActionRetryStrategy::NoRetry => true,
        //         };
        //         if should_remove {
        //             remove_action_ids.push(*action_id);
        //         }
        //     } else {
        //         Self::process_key_value_msg(&self.router, match &slot.action.sub_action {
        //             SubAction::KeyValueServer(action) => {
        //                 KeyValueMsg::KeyValueServer(slot.action.action_id, agent.local_node_id(), agent.local_node_id(), action.clone())
        //             }
        //             SubAction::KeyValueClient(event) => {
        //                 KeyValueMsg::KeyValueClient(slot.action.action_id, agent.local_node_id(), agent.local_node_id(), event.clone())
        //             }
        //         }, &mut self.key_value_server, &mut self.wait_actions, agent);
        //         remove_action_ids.push(*action_id);
        //     }
        // }
        //
        // for action_id in remove_action_ids {
        //     self.wait_actions.remove(&action_id);
        // }
    }

    fn check_incoming_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, agent: &BehaviorAgent<HE, Msg>, conn: Arc<dyn ConnectionSender<Msg>>) -> Option<Box<dyn ConnectionHandler<BE, HE, Msg>>> {
        Some(Box::new(KeyValueConnectionHandler::new(self.router.clone())))
    }

    fn on_outgoing_connection_connected(&mut self, agent: &BehaviorAgent<HE, Msg>, conn: Arc<dyn ConnectionSender<Msg>>) -> Option<Box<dyn ConnectionHandler<BE, HE, Msg>>> {
        Some(Box::new(KeyValueConnectionHandler::new(self.router.clone())))
    }

    fn on_incoming_connection_disconnected(&mut self, agent: &BehaviorAgent<HE, Msg>, conn: Arc<dyn ConnectionSender<Msg>>) {}

    fn on_outgoing_connection_disconnected(&mut self, agent: &BehaviorAgent<HE, Msg>, conn: Arc<dyn ConnectionSender<Msg>>) {}

    fn on_outgoing_connection_error(&mut self, agent: &BehaviorAgent<HE, Msg>, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, agent: &BehaviorAgent<HE, Msg>, node_id: NodeId, conn_id: ConnId, event: BE) {
        if let Ok(msg) = event.try_into() {
            match msg {
                KeyValueBehaviorEvent::FromNode(msg) => {
                    Self::process_key_value_msg(&self.router, msg, &mut self.key_value_server, &mut self.wait_actions, agent);
                }
            }
        }
    }

    fn on_rpc(&mut self, agent: &BehaviorAgent<HE, Msg>, req: Req, res: Box<dyn RpcAnswer<Res>>) -> bool {
        todo!()
    }
}

use crate::handler::KeyValueConnectionHandler;
use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};
use crate::KEY_VALUE_SERVICE_ID;
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::msg::{MsgHeader, TransportMsg};
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
use network::BehaviorAgent;
use parking_lot::RwLock;
use std::sync::Arc;
use utils::Timer;

use self::simple_local::LocalStorage;
use self::simple_remote::RemoteStorage;

mod event_acks;
mod simple_remote;
mod simple_local;
mod sdk;

#[allow(unused)]
pub struct KeyValueBehavior {
    simple_remote: RemoteStorage,
    simple_local: Arc<RwLock<LocalStorage>>,
}

impl KeyValueBehavior {
    #[allow(unused)]
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>, sync_each_ms: u64) -> (Self, sdk::KeyValueSdk) {
        let simple_local = Arc::new(RwLock::new(LocalStorage::new(timer.clone(), sync_each_ms)));
        let sdk = sdk::KeyValueSdk::new(simple_local.clone());

        (Self {
            simple_remote: RemoteStorage::new(timer),
            simple_local,
        }, sdk)
    }

    fn pop_all_events<HE>(&mut self, agent: &BehaviorAgent<HE>) where HE: Send + Sync + 'static {
        while let Some(action) = self.simple_remote.pop_action() {
            agent.send_to_net(TransportMsg::from_payload_bincode(
                MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0),
            &action.0));
        }

        while let Some(action) = self.simple_local.write().pop_action() {
            agent.send_to_net(TransportMsg::from_payload_bincode(
                MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0),
            &action.0));
        }
    }

    fn process_key_value_msg<HE>(&mut self, header: MsgHeader, msg: KeyValueMsg, agent: &BehaviorAgent<HE>) where HE: Send + Sync + 'static,
    {
        match msg {
            KeyValueMsg::Remote(msg) => {
                if let Some(from) = header.from_node {
                    self.simple_remote.on_event(from, msg);
                    self.pop_all_events(agent);
                }
            }
            KeyValueMsg::Local(msg) => {
                if let Some(from) = header.from_node {
                    self.simple_local.write().on_event(from, msg);
                    self.pop_all_events(agent);
                }
            }
        }
    }
}

#[allow(unused)]
impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for KeyValueBehavior
where
    BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        KEY_VALUE_SERVICE_ID
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<HE>, ts_ms: u64, interal_ms: u64) {
        self.simple_remote.tick();
        self.simple_local.write().tick();
        self.pop_all_events(agent);
    }

    fn check_incoming_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, node: NodeId, conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_disconnected(&mut self, agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_error(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, event: BE) {
        if let Ok(msg) = event.try_into() {
            match msg {
                KeyValueBehaviorEvent::FromNode(header, msg) => {
                    self.process_key_value_msg(header, msg, agent);
                }
            }
        }
    }

    fn on_rpc(&mut self, agent: &BehaviorAgent<HE>, req: Req, res: Box<dyn RpcAnswer<Res>>) -> bool {
        false
    }

    fn on_started(&mut self, _agent: &BehaviorAgent<HE>) {}

    fn on_stopped(&mut self, _agent: &BehaviorAgent<HE>) {}
}

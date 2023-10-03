use crate::behavior::awaker::AsyncAwaker;
use crate::handler::KeyValueConnectionHandler;
use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};
use crate::redis::RedisServer;
use crate::KEY_VALUE_SERVICE_ID;
use async_std::task::JoinHandle;
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::msg::{MsgHeader, TransportMsg};
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError};
use network::BehaviorAgent;
use parking_lot::RwLock;
use std::net::SocketAddr;
use std::sync::Arc;
use utils::Timer;

use self::awaker::Awaker;
use self::simple_local::LocalStorage;
use self::simple_remote::RemoteStorage;

mod awaker;
mod event_acks;
mod sdk;
mod simple_local;
mod simple_remote;

pub use sdk::KeyValueSdk;

#[allow(unused)]
pub struct KeyValueBehavior {
    node_id: NodeId,
    simple_remote: RemoteStorage,
    simple_local: Arc<RwLock<LocalStorage>>,
    awake_notify: Arc<dyn Awaker>,
    awake_task: Option<JoinHandle<()>>,
    redis_server: Option<RedisServer>,
    redis_task: Option<JoinHandle<()>>,
}

impl KeyValueBehavior {
    #[allow(unused)]
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>, sync_each_ms: u64, redis_addr: Option<SocketAddr>) -> (Self, sdk::KeyValueSdk) {
        log::info!("[KeyValueBehaviour {}] created with sync_each_ms {}", node_id, sync_each_ms);
        let awake_notify = Arc::new(AsyncAwaker::default());
        let simple_local = Arc::new(RwLock::new(LocalStorage::new(timer.clone(), awake_notify.clone(), sync_each_ms)));
        let sdk = sdk::KeyValueSdk::new(simple_local.clone());

        let sdk_c = sdk.clone();
        let redis_server = redis_addr.map(|addr| RedisServer::new(addr, sdk_c));

        (
            Self {
                node_id,
                simple_remote: RemoteStorage::new(timer),
                simple_local,
                awake_notify,
                awake_task: None,
                redis_server,
                redis_task: None,
            },
            sdk,
        )
    }

    fn pop_all_events<BE, HE>(&mut self, agent: &BehaviorAgent<BE, HE>)
    where
        BE: Send + Sync + 'static,
        HE: Send + Sync + 'static,
    {
        while let Some(action) = self.simple_remote.pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events remote: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            agent.send_to_net(TransportMsg::from_payload_bincode(header, &KeyValueMsg::Local(action.0)));
        }

        while let Some(action) = self.simple_local.write().pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events local: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            agent.send_to_net(TransportMsg::from_payload_bincode(header, &KeyValueMsg::Remote(action.0)));
        }
    }

    fn process_key_value_msg<BE, HE>(&mut self, header: MsgHeader, msg: KeyValueMsg, agent: &BehaviorAgent<BE, HE>)
    where
        BE: Send + Sync + 'static,
        HE: Send + Sync + 'static,
    {
        match msg {
            KeyValueMsg::Remote(msg) => {
                if let Some(from) = header.from_node {
                    log::debug!("[KeyValueBehavior {}] process_key_value_msg remote: {:?} from {}", self.node_id, msg, from);
                    self.simple_remote.on_event(from, msg);
                    self.pop_all_events(agent);
                } else {
                    log::warn!("[KeyValueBehavior {}] process_key_value_msg remote: no from_node", self.node_id);
                }
            }
            KeyValueMsg::Local(msg) => {
                if let Some(from) = header.from_node {
                    log::debug!("[KeyValueBehavior {}] process_key_value_msg local: {:?} from {}", self.node_id, msg, from);
                    self.simple_local.write().on_event(from, msg);
                    self.pop_all_events(agent);
                } else {
                    log::warn!("[KeyValueBehavior {}] process_key_value_msg local: no from_node", self.node_id);
                }
            }
        }
    }
}

#[allow(unused)]
impl<BE, HE> NetworkBehavior<BE, HE> for KeyValueBehavior
where
    BE: From<KeyValueBehaviorEvent> + TryInto<KeyValueBehaviorEvent> + Send + Sync + 'static,
    HE: Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        KEY_VALUE_SERVICE_ID
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<BE, HE>, ts_ms: u64, interal_ms: u64) {
        log::debug!("[KeyValueBehavior {}] on_tick ts_ms {}, interal_ms {}", self.node_id, ts_ms, interal_ms);
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

    fn on_local_event(&mut self, agent: &BehaviorAgent<BE, HE>, _event: BE) {
        self.pop_all_events(agent)
    }

    fn on_local_msg(&mut self, agent: &BehaviorAgent<BE, HE>, msg: TransportMsg) {
        match msg.get_payload_bincode::<KeyValueMsg>() {
            Ok(kv_msg) => {
                log::debug!("[KeyValueBehavior {}] on_local_msg: {:?}", self.node_id, kv_msg);
                self.process_key_value_msg(msg.header, kv_msg, agent);
            }
            Err(e) => {
                log::error!("Error on get_payload_bincode: {:?}", e);
            }
        }
    }

    fn on_incoming_connection_connected(&mut self, agent: &BehaviorAgent<BE, HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_outgoing_connection_connected(&mut self, agent: &BehaviorAgent<BE, HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(KeyValueConnectionHandler::new()))
    }

    fn on_incoming_connection_disconnected(&mut self, agent: &BehaviorAgent<BE, HE>, conn: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_disconnected(&mut self, agent: &BehaviorAgent<BE, HE>, conn: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_error(&mut self, agent: &BehaviorAgent<BE, HE>, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, agent: &BehaviorAgent<BE, HE>, node_id: NodeId, conn_id: ConnId, event: BE) {
        if let Ok(msg) = event.try_into() {
            match msg {
                KeyValueBehaviorEvent::FromNode(header, msg) => {
                    self.process_key_value_msg(header, msg, agent);
                }
                _ => {}
            }
        }
    }

    fn on_started(&mut self, agent: &BehaviorAgent<BE, HE>) {
        log::info!("[KeyValueBehavior {}] on_started", self.node_id);
        let node_id = self.node_id;
        let awake_notify = self.awake_notify.clone();
        let agent = agent.clone();
        self.awake_task = Some(async_std::task::spawn(async move {
            loop {
                awake_notify.wait().await;
                log::debug!("[KeyValueBehavior {}] awake_notify", node_id);
                agent.send_to_behaviour(KeyValueBehaviorEvent::Awake.into());
            }
        }));

        if let Some(mut redis_server) = self.redis_server.take() {
            self.redis_task = Some(async_std::task::spawn(async move {
                redis_server.run().await;
            }));
        }
    }

    fn on_stopped(&mut self, _agent: &BehaviorAgent<BE, HE>) {
        log::info!("[KeyValueBehavior {}] on_stopped", self.node_id);
        if let Some(task) = self.awake_task.take() {
            async_std::task::spawn(async move {
                task.cancel().await;
            });
        }

        if let Some(task) = self.redis_task.take() {
            async_std::task::spawn(async move {
                task.cancel().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{msg::RemoteEvent, KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueMsg, KEY_VALUE_SERVICE_ID};
    use network::{
        behaviour::NetworkBehavior,
        convert_enum,
        plane_tests::{create_mock_behaviour_agent, CrossHandlerGateMockEvent},
    };
    use std::{sync::Arc, time::Duration};
    use utils::MockTimer;

    #[derive(convert_enum::From, convert_enum::TryInto, Debug, PartialEq, Eq)]
    enum BehaviorEvent {
        KeyValue(KeyValueBehaviorEvent),
    }

    #[derive(convert_enum::From, convert_enum::TryInto, Debug, PartialEq, Eq)]
    enum HandlerEvent {
        KeyValue(KeyValueHandlerEvent),
    }

    #[async_std::test]
    async fn sdk_set_should_awake_behaviour() {
        let node_id = 1;
        let sync_ms = 10000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms, None);

        let (mock_agent, _, cross_gate_out) = create_mock_behaviour_agent::<BehaviorEvent, HandlerEvent>(node_id, KEY_VALUE_SERVICE_ID);

        behaviour.on_started(&mock_agent);

        sdk.set(1, vec![1], None);

        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(cross_gate_out.lock().len(), 1);
        let awake_msg = cross_gate_out.lock().pop_front().expect("Should has awake msg");
        assert_eq!(
            awake_msg,
            CrossHandlerGateMockEvent::SendToBehaviour(KEY_VALUE_SERVICE_ID, BehaviorEvent::KeyValue(KeyValueBehaviorEvent::Awake))
        );

        behaviour.on_local_event(&mock_agent, BehaviorEvent::KeyValue(KeyValueBehaviorEvent::Awake));

        //should request to network
        assert_eq!(cross_gate_out.lock().len(), 1);
        let set_msg = cross_gate_out.lock().pop_front().expect("Should has set msg");
        if let CrossHandlerGateMockEvent::SentToNet(msg) = set_msg {
            let msg = msg.get_payload_bincode::<KeyValueMsg>().expect("Should be KeyValueMsg");
            if let KeyValueMsg::Remote(RemoteEvent::Set(_req_id, key_id, value, _version, ex)) = msg {
                assert_eq!(key_id, 1);
                assert_eq!(value, vec![1]);
                assert_eq!(ex, None);
            } else {
                panic!("Should be RemoteEvent::Set")
            }
        } else {
            panic!("Should be SentToNet")
        }

        behaviour.on_stopped(&mock_agent);
    }

    #[async_std::test]
    async fn sdk_get_should_awake_behaviour() {
        let node_id = 1;
        let sync_ms = 10000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms, None);

        let (mock_agent, _, cross_gate_out) = create_mock_behaviour_agent::<BehaviorEvent, HandlerEvent>(node_id, KEY_VALUE_SERVICE_ID);

        behaviour.on_started(&mock_agent);

        let join = async_std::task::spawn(async move {
            sdk.get(1, 10000).await;
        });

        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(cross_gate_out.lock().len(), 1);
        let awake_msg = cross_gate_out.lock().pop_front().expect("Should has awake msg");
        assert_eq!(
            awake_msg,
            CrossHandlerGateMockEvent::SendToBehaviour(KEY_VALUE_SERVICE_ID, BehaviorEvent::KeyValue(KeyValueBehaviorEvent::Awake))
        );

        behaviour.on_local_event(&mock_agent, BehaviorEvent::KeyValue(KeyValueBehaviorEvent::Awake));

        //should request to network
        assert_eq!(cross_gate_out.lock().len(), 1);
        let set_msg = cross_gate_out.lock().pop_front().expect("Should has set msg");
        if let CrossHandlerGateMockEvent::SentToNet(msg) = set_msg {
            let msg = msg.get_payload_bincode::<KeyValueMsg>().expect("Should be KeyValueMsg");
            if let KeyValueMsg::Remote(RemoteEvent::Get(_req_id, key_id)) = msg {
                assert_eq!(key_id, 1);
            } else {
                panic!("Should be RemoteEvent::Set")
            }
        } else {
            panic!("Should be SentToNet")
        }

        behaviour.on_stopped(&mock_agent);
        join.cancel().await;
    }

    #[async_std::test]
    async fn sdk_sub_should_awake_behaviour() {
        let node_id = 1;
        let sync_ms = 10000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms, None);

        let (mock_agent, _, cross_gate_out) = create_mock_behaviour_agent::<BehaviorEvent, HandlerEvent>(node_id, KEY_VALUE_SERVICE_ID);

        behaviour.on_started(&mock_agent);

        let _event_rx = sdk.subscribe(1, None);

        async_std::task::sleep(Duration::from_millis(100)).await;
        assert_eq!(cross_gate_out.lock().len(), 1);
        let awake_msg = cross_gate_out.lock().pop_front().expect("Should has awake msg");
        assert_eq!(
            awake_msg,
            CrossHandlerGateMockEvent::SendToBehaviour(KEY_VALUE_SERVICE_ID, BehaviorEvent::KeyValue(KeyValueBehaviorEvent::Awake))
        );

        behaviour.on_local_event(&mock_agent, BehaviorEvent::KeyValue(KeyValueBehaviorEvent::Awake));

        //should request to network
        assert_eq!(cross_gate_out.lock().len(), 1);
        let set_msg = cross_gate_out.lock().pop_front().expect("Should has set msg");
        if let CrossHandlerGateMockEvent::SentToNet(msg) = set_msg {
            let msg = msg.get_payload_bincode::<KeyValueMsg>().expect("Should be KeyValueMsg");
            if let KeyValueMsg::Remote(RemoteEvent::Sub(_req_id, key_id, ex)) = msg {
                assert_eq!(key_id, 1);
                assert_eq!(ex, None);
            } else {
                panic!("Should be RemoteEvent::Sub")
            }
        } else {
            panic!("Should be SentToNet {:?}", set_msg);
        }

        behaviour.on_stopped(&mock_agent);
    }
}

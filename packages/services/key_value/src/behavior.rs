use crate::handler::KeyValueConnectionHandler;
use crate::msg::{KeyValueBehaviorEvent, KeyValueMsg};
use crate::KEY_VALUE_SERVICE_ID;
use async_std::task::JoinHandle;
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::msg::{MsgHeader, TransportMsg};
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError};
use network::BehaviorAgent;
use parking_lot::RwLock;
use std::sync::Arc;
use utils::awaker::{AsyncAwaker, Awaker};
use utils::Timer;

use self::hashmap_local::HashmapLocalStorage;
use self::hashmap_remote::HashmapRemoteStorage;
use self::simple_local::SimpleLocalStorage;
use self::simple_remote::SimpleRemoteStorage;

mod event_acks;
mod hashmap_local;
mod hashmap_remote;
mod sdk;
mod simple_local;
mod simple_remote;

pub use sdk::KeyValueSdk;

#[allow(unused)]
pub struct KeyValueBehavior {
    node_id: NodeId,
    simple_remote: SimpleRemoteStorage,
    simple_local: Arc<RwLock<SimpleLocalStorage>>,
    hashmap_remote: HashmapRemoteStorage,
    hashmap_local: Arc<RwLock<HashmapLocalStorage>>,
    awake_notify: Arc<dyn Awaker>,
    awake_task: Option<JoinHandle<()>>,
}

impl KeyValueBehavior {
    #[allow(unused)]
    pub fn new(node_id: NodeId, timer: Arc<dyn Timer>, sync_each_ms: u64) -> (Self, sdk::KeyValueSdk) {
        log::info!("[KeyValueBehaviour {}] created with sync_each_ms {}", node_id, sync_each_ms);
        let awake_notify = Arc::new(AsyncAwaker::default());
        let simple_local = Arc::new(RwLock::new(SimpleLocalStorage::new(timer.clone(), awake_notify.clone(), sync_each_ms)));
        let hashmap_local = Arc::new(RwLock::new(HashmapLocalStorage::new(timer.clone(), awake_notify.clone(), sync_each_ms)));
        let sdk = sdk::KeyValueSdk::new(simple_local.clone(), hashmap_local.clone());

        let sdk_c = sdk.clone();

        (
            Self {
                node_id,
                simple_remote: SimpleRemoteStorage::new(timer.clone()),
                simple_local,
                hashmap_remote: HashmapRemoteStorage::new(timer),
                hashmap_local,
                awake_notify,
                awake_task: None,
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
            log::debug!("[KeyValueBehavior {}] pop_all_events simple remote: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            agent.send_to_net(TransportMsg::from_payload_bincode(header, &KeyValueMsg::SimpleLocal(action.0)));
        }

        while let Some(action) = self.simple_local.write().pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events simple local: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            agent.send_to_net(TransportMsg::from_payload_bincode(header, &KeyValueMsg::SimpleRemote(action.0)));
        }

        while let Some(action) = self.hashmap_remote.pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events hashmap remote: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            agent.send_to_net(TransportMsg::from_payload_bincode(header, &KeyValueMsg::HashmapLocal(action.0)));
        }

        while let Some(action) = self.hashmap_local.write().pop_action() {
            log::debug!("[KeyValueBehavior {}] pop_all_events hashmap local: {:?}", self.node_id, action);
            let mut header = MsgHeader::build_reliable(KEY_VALUE_SERVICE_ID, action.1, 0);
            header.from_node = Some(self.node_id);
            agent.send_to_net(TransportMsg::from_payload_bincode(header, &KeyValueMsg::HashmapRemote(action.0)));
        }
    }

    fn process_key_value_msg<BE, HE>(&mut self, header: MsgHeader, msg: KeyValueMsg, agent: &BehaviorAgent<BE, HE>)
    where
        BE: Send + Sync + 'static,
        HE: Send + Sync + 'static,
    {
        match msg {
            KeyValueMsg::SimpleRemote(msg) => {
                if let Some(from) = header.from_node {
                    log::debug!("[KeyValueBehavior {}] process_key_value_msg simple remote: {:?} from {}", self.node_id, msg, from);
                    self.simple_remote.on_event(from, msg);
                    self.pop_all_events(agent);
                } else {
                    log::warn!("[KeyValueBehavior {}] process_key_value_msg simple remote: no from_node", self.node_id);
                }
            }
            KeyValueMsg::SimpleLocal(msg) => {
                if let Some(from) = header.from_node {
                    log::debug!("[KeyValueBehavior {}] process_key_value_msg simple local: {:?} from {}", self.node_id, msg, from);
                    self.simple_local.write().on_event(from, msg);
                    self.pop_all_events(agent);
                } else {
                    log::warn!("[KeyValueBehavior {}] process_key_value_msg simple local: no from_node", self.node_id);
                }
            }
            KeyValueMsg::HashmapRemote(msg) => {
                if let Some(from) = header.from_node {
                    log::debug!("[KeyValueBehavior {}] process_key_value_msg hashmap remote: {:?} from {}", self.node_id, msg, from);
                    self.hashmap_remote.on_event(from, msg);
                    self.pop_all_events(agent);
                } else {
                    log::warn!("[KeyValueBehavior {}] process_key_value_msg hashmap remote: no from_node", self.node_id);
                }
            }
            KeyValueMsg::HashmapLocal(msg) => {
                if let Some(from) = header.from_node {
                    log::debug!("[KeyValueBehavior {}] process_key_value_msg hashmap local: {:?} from {}", self.node_id, msg, from);
                    self.hashmap_local.write().on_event(from, msg);
                    self.pop_all_events(agent);
                } else {
                    log::warn!("[KeyValueBehavior {}] process_key_value_msg hashmap local: no from_node", self.node_id);
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
        log::trace!("[KeyValueBehavior {}] on_tick ts_ms {}, interal_ms {}", self.node_id, ts_ms, interal_ms);
        self.simple_remote.tick();
        self.simple_local.write().tick();
        self.hashmap_remote.tick();
        self.hashmap_local.write().tick();
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
    }

    fn on_stopped(&mut self, _agent: &BehaviorAgent<BE, HE>) {
        log::info!("[KeyValueBehavior {}] on_stopped", self.node_id);
        if let Some(task) = self.awake_task.take() {
            async_std::task::spawn(async move {
                task.cancel().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        msg::{HashmapRemoteEvent, SimpleRemoteEvent},
        KeyValueBehaviorEvent, KeyValueHandlerEvent, KeyValueMsg, KEY_VALUE_SERVICE_ID,
    };
    use network::{
        behaviour::NetworkBehavior,
        convert_enum,
        plane_tests::{create_mock_behaviour_agent, CrossHandlerGateMockEvent},
    };
    use std::{sync::Arc, time::Duration};
    use utils::{error_handle::ErrorUtils, MockTimer};

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
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms);

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
            if let KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Set(_req_id, key_id, value, _version, ex)) = msg {
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
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms);

        let (mock_agent, _, cross_gate_out) = create_mock_behaviour_agent::<BehaviorEvent, HandlerEvent>(node_id, KEY_VALUE_SERVICE_ID);

        behaviour.on_started(&mock_agent);

        let join = async_std::task::spawn(async move {
            sdk.get(1, 10000).await.print_error("");
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
            if let KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Get(_req_id, key_id)) = msg {
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
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms);

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
            if let KeyValueMsg::SimpleRemote(SimpleRemoteEvent::Sub(_req_id, key_id, ex)) = msg {
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

    #[async_std::test]
    async fn sdk_hset_should_awake_behaviour() {
        let node_id = 1;
        let sync_ms = 10000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms);

        let (mock_agent, _, cross_gate_out) = create_mock_behaviour_agent::<BehaviorEvent, HandlerEvent>(node_id, KEY_VALUE_SERVICE_ID);

        behaviour.on_started(&mock_agent);

        sdk.hset(1, 2, vec![1], None);

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
            if let KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Set(_req_id, key_id, sub_key, value, _version, ex)) = msg {
                assert_eq!(key_id, 1);
                assert_eq!(sub_key, 2);
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
    async fn sdk_hget_should_awake_behaviour() {
        let node_id = 1;
        let sync_ms = 10000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms);

        let (mock_agent, _, cross_gate_out) = create_mock_behaviour_agent::<BehaviorEvent, HandlerEvent>(node_id, KEY_VALUE_SERVICE_ID);

        behaviour.on_started(&mock_agent);

        let join = async_std::task::spawn(async move {
            sdk.hget(1, 10000).await.print_error("");
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
            if let KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Get(_req_id, key_id)) = msg {
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
    async fn sdk_hsub_should_awake_behaviour() {
        let node_id = 1;
        let sync_ms = 10000;
        let timer = Arc::new(MockTimer::default());
        let (mut behaviour, sdk) = super::KeyValueBehavior::new(node_id, timer.clone(), sync_ms);

        let (mock_agent, _, cross_gate_out) = create_mock_behaviour_agent::<BehaviorEvent, HandlerEvent>(node_id, KEY_VALUE_SERVICE_ID);

        behaviour.on_started(&mock_agent);

        let _event_rx = sdk.hsubscribe(1, None);

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
            if let KeyValueMsg::HashmapRemote(HashmapRemoteEvent::Sub(_req_id, key_id, ex)) = msg {
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

use crate::handler::ManualHandler;
use crate::msg::*;
use crate::MANUAL_DISCOVERY_SERVICE_ID;
use atm0s_sdn_identity::{ConnId, NodeAddr, NodeAddrType, NodeId};
use atm0s_sdn_key_value::KeyValueSdkEvent;
use atm0s_sdn_key_value::KEY_VALUE_SERVICE_ID;
use atm0s_sdn_network::behaviour::BehaviorContext;
use atm0s_sdn_network::behaviour::NetworkBehaviorAction;
use atm0s_sdn_network::behaviour::{ConnectionHandler, NetworkBehavior};
use atm0s_sdn_network::transport::TransportOutgoingLocalUuid;
use atm0s_sdn_network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError};
use atm0s_sdn_utils::hash::hash_str;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

const CONNECT_WAIT: [u64; 3] = [5000, 10000, 15000];
const CONNECT_WAIT_MAX: u64 = 30000;
const KV_TIMEOUT: u64 = 30000;

const SUB_UUID: u64 = 0x22;

enum OutgoingState {
    New,
    Connecting(u64, Option<ConnId>, usize),
    Connected(u64, ConnId),
    ConnectError(u64, Option<ConnId>, OutgoingConnectionError, usize),
}

struct NodeSlot {
    addr: NodeAddr,
    seed: bool,
    tags: HashMap<u64, ()>,
    incoming: Option<ConnId>,
    outgoing: OutgoingState,
}

pub struct ManualBehaviorConf {
    pub node_id: NodeId,
    pub node_addr: NodeAddr,
    pub seeds: Vec<NodeAddr>,
    pub local_tags: Vec<String>,
    pub connect_tags: Vec<String>,
}

pub struct ManualBehavior<HE, SE> {
    node_id: NodeId,
    node_addr: NodeAddr,
    targets: HashMap<NodeId, NodeSlot>,
    local_tags: Vec<u64>,
    connect_tags: Vec<u64>,
    queue_action: VecDeque<NetworkBehaviorAction<HE, SE>>,
}

impl<HE, SE> ManualBehavior<HE, SE> {
    pub fn new(conf: ManualBehaviorConf) -> Self {
        let mut targets = HashMap::new();
        for addr in conf.seeds {
            if let Some(node_id) = addr.node_id() {
                targets.insert(
                    node_id,
                    NodeSlot {
                        addr,
                        seed: true,
                        tags: HashMap::new(),
                        incoming: None,
                        outgoing: OutgoingState::New,
                    },
                );
            } else {
                log::warn!("[ManualBehavior] Invalid node addr {:?}", addr)
            }
        }
        Self {
            node_id: conf.node_id,
            node_addr: conf.node_addr,
            targets,
            local_tags: conf.local_tags.into_iter().map(|s| hash_str(&s)).collect(),
            connect_tags: conf.connect_tags.into_iter().map(|s| hash_str(&s)).collect(),
            queue_action: VecDeque::new(),
        }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for ManualBehavior<HE, SE>
where
    BE: From<ManualBehaviorEvent> + TryInto<ManualBehaviorEvent> + Send + Sync + 'static,
    HE: From<ManualHandlerEvent> + TryInto<ManualHandlerEvent> + Send + Sync + 'static,
    SE: From<KeyValueSdkEvent> + TryInto<KeyValueSdkEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        MANUAL_DISCOVERY_SERVICE_ID
    }

    fn on_started(&mut self, _context: &BehaviorContext, _now_ms: u64) {
        for tag in &self.local_tags {
            let local_addr = self.node_addr.to_vec();
            log::info!("[MananualBehavior] set tag {}", tag);
            self.queue_action.push_back(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::SetH(*tag, self.node_id as u64, local_addr, Some(KV_TIMEOUT)).into(),
            ));
        }

        for tag in &self.connect_tags {
            log::info!("[MananualBehavior] subsribe tag {}", tag);
            self.queue_action.push_back(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::SubH(SUB_UUID, *tag, Some(KV_TIMEOUT)).into(),
            ));
        }
    }

    fn on_tick(&mut self, _context: &BehaviorContext, now_ms: u64, _interal_ms: u64) {
        for (node_id, slot) in &mut self.targets {
            if slot.incoming.is_none() && (!slot.tags.is_empty() || slot.seed) {
                match &slot.outgoing {
                    OutgoingState::New => {
                        log::info!("[MananualBehavior] connect to node {} addr: {}", node_id, slot.addr);
                        self.queue_action.push_back(NetworkBehaviorAction::ConnectTo(0, *node_id, slot.addr.clone()));
                        slot.outgoing = OutgoingState::Connecting(now_ms, None, 0);
                    }
                    OutgoingState::ConnectError(ts, _conn, _err, count) => {
                        let sleep_ms: &u64 = CONNECT_WAIT.get(*count).unwrap_or(&CONNECT_WAIT_MAX);
                        if ts + *sleep_ms <= now_ms {
                            //need reconnect
                            log::info!("[MananualBehavior] connect to node {} addr: {} after error", node_id, slot.addr);
                            self.queue_action.push_back(NetworkBehaviorAction::ConnectTo(0, *node_id, slot.addr.clone()));
                            slot.outgoing = OutgoingState::Connecting(now_ms, None, count + 1);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, event: SE) {
        if let Ok(kv_event) = event.try_into() {
            match kv_event {
                KeyValueSdkEvent::OnKeyHChanged(_uuid, tag_key, _sub_key, Some(value), _version, source) => {
                    if source == self.node_id {
                        return;
                    }
                    if let Ok(addr) = NodeAddr::try_from(value) {
                        let entry = self.targets.entry(source).or_insert_with(|| {
                            log::info!("[MananualBehavior] new node {} addr {:?} in first tag {}", source, addr, tag_key);
                            NodeSlot {
                                addr,
                                seed: false,
                                tags: HashMap::new(),
                                incoming: None,
                                outgoing: OutgoingState::New,
                            }
                        });
                        if entry.tags.insert(tag_key, ()).is_none() {
                            log::info!("[MananualBehavior] node {} addr {:?} added tag {}", source, entry.addr, tag_key);
                        }
                    }
                }
                KeyValueSdkEvent::OnKeyHChanged(_uuid, tag_key, _sub_key, None, _version, source) => {
                    if source == self.node_id {
                        return;
                    }
                    if let Some(entry) = self.targets.get_mut(&source) {
                        if entry.tags.remove(&tag_key).is_some() {
                            log::info!("[MananualBehavior] node {} addr {:?} removed tag {}", source, entry.addr, tag_key);
                        }
                        if entry.tags.is_empty() {
                            log::info!("[MananualBehavior] node {} addr {:?} removed in last tag {} => close conn", source, entry.addr, tag_key);
                            if !entry.seed {
                                self.targets.remove(&source);
                            }
                            self.queue_action.push_back(NetworkBehaviorAction::CloseNode(source));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn check_incoming_connection(&mut self, _context: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _context: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId, _local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
        if let Some(neighbour) = self.targets.get_mut(&node) {
            match neighbour.outgoing {
                OutgoingState::New => {
                    neighbour.outgoing = OutgoingState::Connecting(now_ms, Some(conn_id), 0);
                    Ok(())
                }
                OutgoingState::Connecting(_, _, count) => {
                    neighbour.outgoing = OutgoingState::Connecting(now_ms, Some(conn_id), count);
                    Ok(())
                }
                _ => Err(ConnectionRejectReason::ConnectionLimited),
            }
        } else {
            Ok(())
        }
    }

    fn on_local_msg(&mut self, _context: &BehaviorContext, _now_ms: u64, _msg: atm0s_sdn_network::msg::TransportMsg) {
        panic!("Should not happend");
    }

    fn on_incoming_connection_connected(&mut self, _context: &BehaviorContext, _now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        let entry = self.targets.entry(conn.remote_node_id()).or_insert_with(|| NodeSlot {
            addr: conn.remote_addr(),
            tags: HashMap::new(),
            seed: false,
            incoming: None,
            outgoing: OutgoingState::New,
        });
        entry.incoming = Some(conn.conn_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _context: &BehaviorContext,
        now_ms: u64,
        connection: Arc<dyn ConnectionSender>,
        _local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        let entry = self.targets.entry(connection.remote_node_id()).or_insert_with(|| NodeSlot {
            addr: connection.remote_addr(),
            tags: HashMap::new(),
            seed: false,
            incoming: None,
            outgoing: OutgoingState::New,
        });
        entry.outgoing = OutgoingState::Connected(now_ms, connection.conn_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_incoming_connection_disconnected(&mut self, _context: &BehaviorContext, _now_ms: u64, node: NodeId, _conn_id: ConnId) {
        if let Some(slot) = self.targets.get_mut(&node) {
            slot.incoming = None;
        }
    }

    fn on_outgoing_connection_disconnected(&mut self, _context: &BehaviorContext, _now_ms: u64, node: NodeId, _conn_id: ConnId) {
        if let Some(slot) = self.targets.get_mut(&node) {
            slot.outgoing = OutgoingState::New;
        }
    }

    fn on_outgoing_connection_error(
        &mut self,
        _context: &BehaviorContext,
        now_ms: u64,
        node_id: NodeId,
        conn_id: Option<ConnId>,
        _local_uuid: TransportOutgoingLocalUuid,
        err: &OutgoingConnectionError,
    ) {
        if let Some(slot) = self.targets.get_mut(&node_id) {
            if let OutgoingState::Connecting(_, _, count) = slot.outgoing {
                slot.outgoing = OutgoingState::ConnectError(now_ms, conn_id, err.clone(), count);
            }
        }
    }

    fn on_handler_event(&mut self, _context: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _connection_id: ConnId, _event: BE) {}

    fn on_stopped(&mut self, _context: &BehaviorContext, _now_ms: u64) {
        for tag in &self.local_tags {
            self.queue_action
                .push_back(NetworkBehaviorAction::ToSdkService(KEY_VALUE_SERVICE_ID, KeyValueSdkEvent::DelH(*tag, self.node_id as u64).into()));
        }

        for tag in &self.connect_tags {
            self.queue_action
                .push_back(NetworkBehaviorAction::ToSdkService(KEY_VALUE_SERVICE_ID, KeyValueSdkEvent::UnsubH(SUB_UUID, *tag).into()));
        }
    }

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        self.queue_action.pop_front()
    }
}

#[cfg(test)]
mod test {
    use std::{str::FromStr, sync::Arc};

    use atm0s_sdn_identity::{ConnId, NodeAddr, NodeId};
    use atm0s_sdn_key_value::{KeyValueSdkEvent, KEY_VALUE_SERVICE_ID};
    use atm0s_sdn_network::{
        behaviour::{BehaviorContext, NetworkBehavior, NetworkBehaviorAction},
        transport::OutgoingConnectionError,
    };
    use atm0s_sdn_utils::{awaker::MockAwaker, hash::hash_str};

    use crate::{
        behavior::{CONNECT_WAIT, KV_TIMEOUT, SUB_UUID},
        ManualBehavior, ManualBehaviorConf, ManualBehaviorEvent, ManualHandlerEvent, MANUAL_DISCOVERY_SERVICE_ID,
    };

    type BE = ManualBehaviorEvent;
    type HE = ManualHandlerEvent;
    type SE = KeyValueSdkEvent;

    #[test]
    fn connect_to_seed() {
        let node_id = 1;
        let node_addr = NodeAddr::from_str("/p2p/1").expect("");

        let seed_addr = NodeAddr::from_str("/p2p/2").expect("");

        let ctx = BehaviorContext {
            node_id,
            awaker: Arc::new(MockAwaker::default()),
            service_id: MANUAL_DISCOVERY_SERVICE_ID,
        };

        let mut behaviour = ManualBehavior::<HE, SE>::new(ManualBehaviorConf {
            node_id,
            node_addr,
            seeds: vec![seed_addr.clone()],
            local_tags: vec![],
            connect_tags: vec![],
        });

        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        behaviour.on_started(&ctx, 0);
        behaviour.on_tick(&ctx, 100, 3000);

        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ConnectTo(0, 2, seed_addr.clone())));
        assert_eq!(behaviour.pop_action(), None);

        // connect error should reconnect
        behaviour.on_outgoing_connection_error(&ctx, 10000, 2, None, 0, &OutgoingConnectionError::AuthenticationError);
        behaviour.on_tick(&ctx, 10000, 3000);

        assert_eq!(behaviour.pop_action(), None);

        //after wait CONNECT_WAIT[0] ms should connect
        behaviour.on_tick(&ctx, 10000 + CONNECT_WAIT[0], 3000);
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ConnectTo(0, 2, seed_addr.clone())));
        assert_eq!(behaviour.pop_action(), None);
    }

    #[test]
    fn init_with_local_tags() {
        let node_id = 1;
        let node_addr = NodeAddr::from_str("/p2p/1").expect("");

        let ctx = BehaviorContext {
            node_id,
            awaker: Arc::new(MockAwaker::default()),
            service_id: MANUAL_DISCOVERY_SERVICE_ID,
        };

        let mut behaviour = ManualBehavior::<HE, SE>::new(ManualBehaviorConf {
            node_id,
            node_addr: node_addr.clone(),
            seeds: vec![],
            local_tags: vec!["demo".to_string()],
            connect_tags: vec![],
        });

        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        behaviour.on_started(&ctx, 0);
        assert_eq!(
            behaviour.pop_action(),
            Some(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::SetH(hash_str("demo"), node_id as u64, node_addr.to_vec(), Some(KV_TIMEOUT))
            ))
        );
        assert_eq!(behaviour.pop_action(), None);

        // on event from this own tag => should not connect to node
        behaviour.on_sdk_msg(
            &ctx,
            0,
            KEY_VALUE_SERVICE_ID,
            KeyValueSdkEvent::OnKeyHChanged(SUB_UUID, hash_str("demo"), node_id as u64, Some(node_addr.to_vec()), 0, node_id).into(),
        );
        behaviour.on_tick(&ctx, 100, 3000);
        assert_eq!(behaviour.pop_action(), None);
    }

    #[test]
    fn init_with_remote_tags() {
        let node_id = 1;
        let node_addr = NodeAddr::from_str("/p2p/1").expect("");

        let remote_id = 2;
        let remote_addr = NodeAddr::from_str("/p2p/2").expect("");

        let ctx = BehaviorContext {
            node_id,
            awaker: Arc::new(MockAwaker::default()),
            service_id: MANUAL_DISCOVERY_SERVICE_ID,
        };

        let mut behaviour = ManualBehavior::<HE, SE>::new(ManualBehaviorConf {
            node_id,
            node_addr: node_addr.clone(),
            seeds: vec![],
            local_tags: vec![],
            connect_tags: vec!["demo".to_string()],
        });

        let behaviour: &mut dyn NetworkBehavior<BE, HE, SE> = &mut behaviour;

        behaviour.on_started(&ctx, 0);
        assert_eq!(
            behaviour.pop_action(),
            Some(NetworkBehaviorAction::ToSdkService(
                KEY_VALUE_SERVICE_ID,
                KeyValueSdkEvent::SubH(SUB_UUID, hash_str("demo"), Some(KV_TIMEOUT))
            ))
        );
        assert_eq!(behaviour.pop_action(), None);

        // on kv changed => should connect to node
        behaviour.on_sdk_msg(
            &ctx,
            0,
            KEY_VALUE_SERVICE_ID,
            KeyValueSdkEvent::OnKeyHChanged(SUB_UUID, hash_str("demo"), remote_id as u64, Some(remote_addr.to_vec()), 0, remote_id).into(),
        );
        behaviour.on_tick(&ctx, 100, 3000);
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::ConnectTo(0, remote_id, remote_addr.clone())));
        assert_eq!(behaviour.pop_action(), None);

        // on kv changed => should disconnect from node
        behaviour.on_sdk_msg(
            &ctx,
            0,
            KEY_VALUE_SERVICE_ID,
            KeyValueSdkEvent::OnKeyHChanged(SUB_UUID, hash_str("demo"), remote_id as u64, None, 0, remote_id).into(),
        );
        assert_eq!(behaviour.pop_action(), Some(NetworkBehaviorAction::CloseNode(remote_id)));
        assert_eq!(behaviour.pop_action(), None);
    }
}

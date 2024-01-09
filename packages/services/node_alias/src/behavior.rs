use std::{sync::Arc, collections::VecDeque};

use async_std::task::JoinHandle;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_network::{
    behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior, NetworkBehaviorAction},
    msg::{TransportMsg, MsgHeader},
    transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError},
};
use atm0s_sdn_pub_sub::{PubsubSdk, Publisher};
use atm0s_sdn_router::RouteRule;
use bytes::Bytes;
use parking_lot::Mutex;

use crate::{handler::NodeAliasHandler, NODE_ALIAS_SERVICE_ID, msg::{BroadcastMsg, SdkControl}, internal::{ServiceInternal, ServiceInternalAction}, sdk::NodeAliasSdk};

const NODE_ALIAS_BROADCAST_CHANNEL: u32 = 0x13ba2c; //TODO hash of "atm0s.node_alias.broadcast"

pub struct NodeAliasBehavior {
    node_id: NodeId,
    pubsub_sdk: PubsubSdk,
    pub_channel: Publisher,
    pubsub_task: Option<JoinHandle<()>>,
    incomming_broadcast_queue: Arc<Mutex<VecDeque<(NodeId, BroadcastMsg)>>>,
    sdk: NodeAliasSdk,
    internal: Arc<Mutex<ServiceInternal>>,
}

impl NodeAliasBehavior {
    pub fn new(node_id: NodeId, pubsub_sdk: PubsubSdk) -> (Self, NodeAliasSdk) {
        let sdk = NodeAliasSdk::default();
        let instance = Self {
            node_id,
            pub_channel: pubsub_sdk.create_publisher(NODE_ALIAS_BROADCAST_CHANNEL),
            pubsub_sdk,            
            pubsub_task: None,
            incomming_broadcast_queue: Arc::new(Mutex::new(VecDeque::new())),
            sdk: sdk.clone(),
            internal: Arc::new(Mutex::new(ServiceInternal::new(node_id))),
        };
        
        (instance, sdk)
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for NodeAliasBehavior {
    fn service_id(&self) -> u8 {
        NODE_ALIAS_SERVICE_ID
    }

    fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
        self.sdk.set_awaker(ctx.awaker.clone());
        let node_id = self.node_id;
        let sub_channel = self.pubsub_sdk.create_consumer(NODE_ALIAS_BROADCAST_CHANNEL, None);
        let awaker = ctx.awaker.clone();
        let incomming_broadcast_msg = self.incomming_broadcast_queue.clone();
        self.pubsub_task = Some(async_std::task::spawn(async move {
            loop {
                if let Some((_, source, _, msg)) = sub_channel.recv().await {
                    if source == node_id {
                        continue;
                    }
                    if let Ok(msg) = bincode::deserialize(&msg) {
                        incomming_broadcast_msg.lock().push_back((source, msg));
                        awaker.notify();
                    }
                }
            }
        }));
    }

    fn on_tick(&mut self, _ctx: &BehaviorContext, now_ms: u64, _interval_ms: u64) {
        self.internal.lock().on_tick(now_ms);
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, now_ms: u64) {
        let mut incomming_broadcast_msg = self.incomming_broadcast_queue.lock();
        while let Some((source, msg)) = incomming_broadcast_msg.pop_front() {
            self.internal.lock().on_incomming_broadcast(now_ms, source, msg);
        }
        while let Some(msg) = self.sdk.pop_control() {
            match msg {
                SdkControl::Register(alias) => {
                    self.internal.lock().register(now_ms, alias);
                },
                SdkControl::Unregister(alias) => {
                    self.internal.lock().unregister(now_ms, &alias);
                },
                SdkControl::Query(alias, sender) => {
                    self.internal.lock().find_alias(now_ms, &alias, sender);
                }
            }
        }
    }

    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _msg: TransportMsg) {
        panic!("Should not happend");
    }

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(NodeAliasHandler { internal: self.internal.clone() }))
    }

    fn on_outgoing_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(NodeAliasHandler { internal: self.internal.clone() }))
    }

    fn on_incoming_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {
        
    }

    fn on_outgoing_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {
        
    }

    fn on_outgoing_connection_error(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _err: &OutgoingConnectionError) {
        
    }

    fn on_handler_event(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {
        if let Some(task) = self.pubsub_task.take() {
            async_std::task::spawn(async move {
                task.cancel().await;
            });
        }
    }

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
        match self.internal.lock().pop_action() {
            Some(ServiceInternalAction::Broadcast(msg)) => {
                log::info!("[NodeAliasBehavior {}] Broadcasting: {:?}", self.node_id, msg);
                let msg = bincode::serialize(&msg).unwrap();
                self.pub_channel.send(Bytes::from(msg));
                None
            },
            Some(ServiceInternalAction::Unicast(dest, msg)) => {
                log::info!("[NodeAliasBehavior {}] Unicasting to {}: {:?}", self.node_id, dest, msg);
                let header = MsgHeader::build(NODE_ALIAS_SERVICE_ID, NODE_ALIAS_SERVICE_ID, RouteRule::ToNode(dest))
                    .set_from_node(Some(self.node_id));
                Some(NetworkBehaviorAction::ToNet(TransportMsg::from_payload_bincode(header, &msg)))
            },
            None => None,
        }
    }
}

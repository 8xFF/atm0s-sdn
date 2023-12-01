use atm0s_sdn_identity::NodeId;
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{marker::PhantomData, sync::Arc};

use async_std::channel::{Receiver, Sender};
use atm0s_sdn_utils::Timer;

use behaviour::RpcBoxBehaviour;
use emitter::RpcBoxEmitter;

use crate::msg::RpcTransmitEvent;

use self::logic::RpcLogic;

pub(super) mod behaviour;
pub(super) mod emitter;
pub(super) mod handler;
pub(super) mod logic;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RpcError {
    DestinationNotFound,
    ServiceNotFound,
    Timeout,
    LocalQueueError,
    RemoteQueueError,
    DeserializeError,
    RuntimeError(String),
}

pub struct RpcResponse<Res> {
    _tmp: PhantomData<Res>,
    dest_service_id: u8,
    dest_node: NodeId,
    req_id: u64,
    logic: Arc<RwLock<RpcLogic>>,
}

impl<Res: Serialize> RpcResponse<Res> {
    pub fn success(self, res: Res) {
        self.logic
            .write()
            .send_answer(self.dest_service_id, self.dest_node, self.req_id, Ok(bincode::serialize(&res).expect("Should convert to buf")));
    }

    pub fn error(self, err: &str) {
        self.logic
            .write()
            .send_answer(self.dest_service_id, self.dest_node, self.req_id, Err(RpcError::RuntimeError(err.to_string())));
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RpcRequest<Req> {
    pub dest_service_id: u8,
    pub dest_node: NodeId,
    pub req_id: u64,
    pub req: Req,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RpcBoxEvent<E, Req> {
    Event(E),
    Request(RpcRequest<Req>),
}

/// Implement RPC method over atm0s-sdn
/// RPC can be added to inside a service like:
///
/// ```rust
/// use std::sync::Arc;
/// use atm0s_sdn_network::behaviour::BehaviorContext;
/// use atm0s_sdn_router::RouteRule;
/// use atm0s_sdn_utils::{SystemTimer};
/// use atm0s_sdn_rpc::{RpcBox, RpcBoxEvent};
///
/// type E = u8;
/// type Req = u8;
/// type Res = u8;
///
/// async fn test() {
///     let node_id = 1;
///     let service_id = 100;
///     let timer = Arc::new(SystemTimer());
///     let mut rpc_box = RpcBox::<E, Req, Res>::new(node_id, service_id, timer);
///     let rpc_behaviour = rpc_box.behaviour();
///     
///     // Then use rpc_behaviour inside service behaviour
///     // fn on_started(&mut self, ctx: &BehaviorContext, _now_ms: u64) {
///     //    self.rpc_behaviour.set_awaker(ctx.awaker.clone());
///     // }
///     //
///     // fn on_tick(&mut self, _ctx: &BehaviorContext, now_ms: u64, _interval_ms: u64) {
///     //    self.rpc_behaviour.on_tick(now_ms);
///     // }
///     // fn on_awake(&mut self, _ctx: &BehaviorContext, now_ms: u64) {
///     //    self.logic.on_tick(now_ms);
///     // }
///     // fn on_local_msg(&mut self, _ctx: &BehaviorContext, now_ms: u64, msg: TransportMsg) {
///     //    self.logic.on_msg(now_ms, msg);
///     // }
///     // fn on_outgoing_connection_connected(
///     //    &mut self,
///     //    _ctx: &BehaviorContext,
///     //    _now_ms: u64,
///     //    _conn: Arc<dyn ConnectionSender>,
///     //    _local_uuid: TransportOutgoingLocalUuid,
///     // ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
///     //    Some(Box::new(RpcHandler {
///     //      logic: self.logic.create_handler(),
///     //    }))
///     // }
///     //
///     // fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE, SE>> {
///     //     self.logic.pop_net_out().map(|msg| NetworkBehaviorAction::ToNet(msg))
///     // }
///     
///     // Then in handler we need
///     // fn on_event(&mut self, _ctx: &ConnectionContext, now_ms: u64, event: ConnectionEvent) {
///     //     if let ConnectionEvent::Msg(msg) = event {
///     //         self.logic.on_msg(now_ms, msg);
///     //      }
///     // }
///     
///     // After that we can send event, request and hander incoming like
///     
///     let emitter = rpc_box.emitter();
///     
///     // To send a event to service 100 with router rule ToService
///     emitter.emit(100, RouteRule::ToService(0), 111);
///     
///     // To send a request and wait response
///     let res = emitter.request(100, RouteRule::ToService(0), 111, 10000).await;
///     
///     // To recv rpc event in current node
///     while let Some(event) = rpc_box.recv().await {
///         match event {
///             RpcBoxEvent::Event(event) => {},
///             RpcBoxEvent::Request(req) => {
///                 let res = rpc_box.response_for(&req);
///                 res.success(222);
///             }
///         }
///     }
/// }    
/// ```
///
pub struct RpcBox<E, Req, Res> {
    _tmp: PhantomData<(E, Req, Res)>,
    tx: Sender<(NodeId, RpcTransmitEvent)>,
    rx: Receiver<(NodeId, RpcTransmitEvent)>,
    service_id: u8,
    node_id: NodeId,
    logic: Arc<RwLock<RpcLogic>>,
}

impl<E: DeserializeOwned, Req: DeserializeOwned, Res: Serialize> RpcBox<E, Req, Res> {
    pub fn new(node_id: NodeId, service_id: u8, timer: Arc<dyn Timer>) -> Self {
        let (tx, rx) = async_std::channel::bounded(100);
        Self {
            _tmp: Default::default(),
            tx,
            rx,
            node_id,
            service_id,
            logic: Arc::new(RwLock::new(RpcLogic::new(timer))),
        }
    }

    pub fn emitter(&mut self) -> RpcBoxEmitter<E, Req, Res> {
        RpcBoxEmitter {
            _tmp: Default::default(),
            logic: self.logic.clone(),
        }
    }

    pub fn behaviour(&mut self) -> RpcBoxBehaviour {
        RpcBoxBehaviour {
            logic: self.logic.clone(),
            node_id: self.node_id,
            service_id: self.service_id,
            tx: self.tx.clone(),
        }
    }

    pub fn response_for(&self, req: &RpcRequest<Req>) -> RpcResponse<Res> {
        RpcResponse {
            _tmp: Default::default(),
            dest_node: req.dest_node,
            dest_service_id: req.dest_service_id,
            logic: self.logic.clone(),
            req_id: req.req_id,
        }
    }

    pub async fn recv(&mut self) -> Option<RpcBoxEvent<E, Req>> {
        loop {
            let (from_node, event) = self.rx.recv().await.ok()?;
            if let Some(event) = self.convert(from_node, event) {
                break Some(event);
            }
        }
    }

    pub fn try_recv(&mut self) -> Option<RpcBoxEvent<E, Req>> {
        let (from_node, event) = self.rx.try_recv().ok()?;
        self.convert(from_node, event)
    }

    fn convert(&self, from_node: NodeId, event: RpcTransmitEvent) -> Option<RpcBoxEvent<E, Req>> {
        match event {
            RpcTransmitEvent::Event(_from_service, buf) => {
                if let Ok(event) = bincode::deserialize(&buf) {
                    Some(RpcBoxEvent::Event(event))
                } else {
                    None
                }
            }
            RpcTransmitEvent::Request(from_service, req_id, buf) => {
                if let Ok(req) = bincode::deserialize(&buf) {
                    let req = RpcRequest {
                        req_id,
                        req,
                        dest_node: from_node,
                        dest_service_id: from_service,
                    };
                    Some(RpcBoxEvent::Request(req))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

use crate::handler::LayersSpreadRouterSyncHandler;
use crate::mgs::{LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use crate::FAST_PATH_ROUTE_SERVICE_ID;
use atm0s_sdn_identity::{ConnId, NodeId};
use atm0s_sdn_layers_spread_router::SharedRouter;
use atm0s_sdn_network::behaviour::{BehaviorContext, ConnectionHandler, NetworkBehavior};
use atm0s_sdn_network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError};
use std::sync::Arc;

pub struct LayersSpreadRouterSyncBehavior {
    pub router: SharedRouter,
}

impl LayersSpreadRouterSyncBehavior {
    pub fn new(router: SharedRouter) -> Self {
        Self { router }
    }
}

impl<BE, HE, SE> NetworkBehavior<BE, HE, SE> for LayersSpreadRouterSyncBehavior
where
    BE: From<LayersSpreadRouterSyncBehaviorEvent> + TryInto<LayersSpreadRouterSyncBehaviorEvent> + Send + Sync + 'static,
    HE: From<LayersSpreadRouterSyncHandlerEvent> + TryInto<LayersSpreadRouterSyncHandlerEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        FAST_PATH_ROUTE_SERVICE_ID
    }

    fn on_tick(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _interal_ms: u64) {}

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}
    fn on_sdk_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _from_service: u8, _event: SE) {}

    fn check_incoming_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_local_msg(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _msg: atm0s_sdn_network::msg::TransportMsg) {
        panic!("Should not happend");
    }

    fn on_incoming_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(LayersSpreadRouterSyncHandler::new(self.router.clone())))
    }

    fn on_outgoing_connection_connected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(LayersSpreadRouterSyncHandler::new(self.router.clone())))
    }

    fn on_incoming_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_disconnected(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId) {}

    fn on_outgoing_connection_error(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, _ctx: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_started(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn on_stopped(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<atm0s_sdn_network::behaviour::NetworkBehaviorAction<HE, SE>> {
        None
    }
}

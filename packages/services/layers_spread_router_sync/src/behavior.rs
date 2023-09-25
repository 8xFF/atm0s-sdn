use crate::handler::LayersSpreadRouterSyncHandler;
use crate::mgs::{LayersSpreadRouterSyncBehaviorEvent, LayersSpreadRouterSyncHandlerEvent};
use crate::FAST_PATH_ROUTE_SERVICE_ID;
use bluesea_identity::{ConnId, NodeId};
use layers_spread_router::SharedRouter;
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
use network::BehaviorAgent;
use std::sync::Arc;

pub struct LayersSpreadRouterSyncBehavior {
    pub router: SharedRouter,
}

impl LayersSpreadRouterSyncBehavior {
    pub fn new(router: SharedRouter) -> Self {
        Self { router }
    }
}

impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for LayersSpreadRouterSyncBehavior
where
    BE: From<LayersSpreadRouterSyncBehaviorEvent> + TryInto<LayersSpreadRouterSyncBehaviorEvent> + Send + Sync + 'static,
    HE: From<LayersSpreadRouterSyncHandlerEvent> + TryInto<LayersSpreadRouterSyncHandlerEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        FAST_PATH_ROUTE_SERVICE_ID
    }

    fn on_tick(&mut self, _agent: &BehaviorAgent<HE>, _ts_ms: u64, _interal_ms: u64) {
        self.router.dump();
    }

    fn check_incoming_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(LayersSpreadRouterSyncHandler::new(self.router.clone())))
    }

    fn on_outgoing_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, _conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        Some(Box::new(LayersSpreadRouterSyncHandler::new(self.router.clone())))
    }

    fn on_incoming_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, _conn: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, _conn: Arc<dyn ConnectionSender>) {}

    fn on_outgoing_connection_error(&mut self, _agent: &BehaviorAgent<HE>, _node_id: NodeId, _conn_id: ConnId, _err: &OutgoingConnectionError) {}

    fn on_handler_event(&mut self, _agent: &BehaviorAgent<HE>, _node_id: NodeId, _conn_id: ConnId, _event: BE) {}

    fn on_rpc(&mut self, _agent: &BehaviorAgent<HE>, _req: Req, res: Box<dyn RpcAnswer<Res>>) -> bool {
        res.error(0, "NOT_IMPLEMENTED");
        true
    }
}

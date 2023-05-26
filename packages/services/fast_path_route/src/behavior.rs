use crate::handler::FastPathRouteHandler;
use crate::mgs::{FastPathRouteBehaviorEvent, FastPathRouteHandlerEvent, FastPathRouteMsg};
use crate::FAST_PATH_ROUTE_SERVICE_ID;
use bluesea_identity::{ConnId, NodeId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::transport::{
    ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer,
};
use network::BehaviorAgent;
use router::SharedRouter;
use std::sync::Arc;

pub struct FastPathRouteBehavior {
    pub router: SharedRouter,
}

impl FastPathRouteBehavior {
    pub fn new(router: SharedRouter) -> Self {
        Self { router }
    }
}

impl<BE, HE, Msg, Req, Res> NetworkBehavior<BE, HE, Msg, Req, Res> for FastPathRouteBehavior
where
    BE: From<FastPathRouteBehaviorEvent>
        + TryInto<FastPathRouteBehaviorEvent>
        + Send
        + Sync
        + 'static,
    HE: From<FastPathRouteHandlerEvent>
        + TryInto<FastPathRouteHandlerEvent>
        + Send
        + Sync
        + 'static,
    Msg: From<FastPathRouteMsg> + TryInto<FastPathRouteMsg> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        FAST_PATH_ROUTE_SERVICE_ID
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<HE, Msg>, ts_ms: u64, interal_ms: u64) {
        self.router.dump();
    }

    fn check_incoming_connection(
        &mut self,
        node: NodeId,
        conn_id: ConnId,
    ) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(
        &mut self,
        node: NodeId,
        conn_id: ConnId,
    ) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, Msg>,
        conn: Arc<dyn ConnectionSender<Msg>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, Msg>>> {
        Some(Box::new(FastPathRouteHandler::new(self.router.clone())))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, Msg>,
        conn: Arc<dyn ConnectionSender<Msg>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, Msg>>> {
        Some(Box::new(FastPathRouteHandler::new(self.router.clone())))
    }

    fn on_incoming_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, Msg>,
        conn: Arc<dyn ConnectionSender<Msg>>,
    ) {
    }

    fn on_outgoing_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, Msg>,
        conn: Arc<dyn ConnectionSender<Msg>>,
    ) {
    }

    fn on_outgoing_connection_error(
        &mut self,
        agent: &BehaviorAgent<HE, Msg>,
        node_id: NodeId,
        conn_id: ConnId,
        err: &OutgoingConnectionError,
    ) {
    }

    fn on_handler_event(
        &mut self,
        agent: &BehaviorAgent<HE, Msg>,
        node_id: NodeId,
        conn_id: ConnId,
        event: BE,
    ) {
    }

    fn on_rpc(
        &mut self,
        agent: &BehaviorAgent<HE, Msg>,
        req: Req,
        res: Box<dyn RpcAnswer<Res>>,
    ) -> bool {
        res.error(0, "NOT_IMPLEMENTED");
        true
    }
}

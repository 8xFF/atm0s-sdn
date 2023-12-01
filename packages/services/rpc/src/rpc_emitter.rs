use std::sync::Arc;

use async_std::channel::{bounded, Sender};
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::Timer;
use parking_lot::Mutex;

use crate::{
    rpc_box::RpcResponse,
    rpc_msg::{RpcError, RpcMsg},
    rpc_queue::RpcQueue,
};

pub struct RpcEmitter {
    pub(crate) timer: Arc<dyn Timer>,
    pub(crate) rpc_queue: Arc<Mutex<RpcQueue<Sender<Result<RpcMsg, RpcError>>>>>,
}

impl RpcEmitter {
    pub fn emit<E: Into<Vec<u8>>>(&self, to_service: u8, rule: RouteRule, cmd: &str, event: E) {
        self.rpc_queue.lock().add_event(to_service, rule, cmd, event);
    }

    pub async fn request<Req: Into<Vec<u8>>, Res: TryFrom<Vec<u8>>>(&self, to_service: u8, rule: RouteRule, cmd: &str, req: Req, timeout_ms: u64) -> Result<Res, RpcError> {
        let (tx, rx) = bounded(1);
        self.rpc_queue.lock().add_request(self.timer.now_ms(), to_service, rule, cmd, req, tx, timeout_ms);
        let res = rx.recv().await.map_err(|_| RpcError::LocalQueueError)??;
        res.take_answer().ok_or(RpcError::DeserializeError)?.1
    }

    pub fn response_for<Res: Into<Vec<u8>>>(&self, req: RpcMsg) -> RpcResponse<Res> {
        RpcResponse {
            _tmp: Default::default(),
            req,
            rpc_queue: self.rpc_queue.clone(),
        }
    }
}

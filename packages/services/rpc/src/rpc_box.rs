use std::sync::Arc;

use async_std::channel::{Receiver, Sender};
use atm0s_sdn_identity::NodeId;
use atm0s_sdn_utils::Timer;
use parking_lot::Mutex;

use crate::{
    rpc_emitter::RpcEmitter,
    rpc_msg::{RpcError, RpcMsg},
    rpc_queue::RpcQueue,
    RpcBehavior,
};

pub struct RpcRequest<Param: for<'a> TryFrom<&'a [u8]>, Res: Into<Vec<u8>>> {
    pub(crate) _tmp: Option<Res>,
    pub(crate) req: RpcMsg,
    pub(crate) param: Param,
    pub(crate) rpc_queue: Arc<Mutex<RpcQueue<Sender<Result<RpcMsg, RpcError>>>>>,
}

impl<Param: for<'a> TryFrom<&'a [u8]>, Res: Into<Vec<u8>>> RpcRequest<Param, Res> {
    pub fn param(&self) -> &Param {
        &self.param
    }

    pub fn answer(&self, res: Result<Res, RpcError>) {
        self.rpc_queue.lock().answer_for(&self.req, res);
    }

    pub fn success(&self, res: Res) {
        self.rpc_queue.lock().answer_for(&self.req, Ok(res));
    }

    pub fn error(&self, err: &str) {
        self.rpc_queue.lock().answer_for::<Res>(&self.req, Err(RpcError::RuntimeError(err.to_string())));
    }
}

pub struct RpcBox {
    tx: Sender<RpcMsg>,
    rx: Receiver<RpcMsg>,
    service_id: u8,
    timer: Arc<dyn Timer>,
    rpc_queue: Arc<Mutex<RpcQueue<Sender<Result<RpcMsg, RpcError>>>>>,
}

impl RpcBox {
    pub fn new(node_id: NodeId, service_id: u8, timer: Arc<dyn Timer>) -> Self {
        let (tx, rx) = async_std::channel::bounded(100);
        Self {
            tx,
            rx,
            service_id,
            timer,
            rpc_queue: Arc::new(Mutex::new(RpcQueue::new(node_id, service_id))),
        }
    }

    pub fn emitter(&mut self) -> RpcEmitter {
        RpcEmitter {
            timer: self.timer.clone(),
            rpc_queue: self.rpc_queue.clone(),
        }
    }

    pub fn behaviour(&mut self) -> RpcBehavior {
        RpcBehavior {
            service_id: self.service_id,
            rpc_queue: self.rpc_queue.clone(),
            tx: self.tx.clone(),
        }
    }

    pub async fn recv(&mut self) -> Option<RpcMsg> {
        self.rx.recv().await.ok()
    }

    pub fn try_recv(&mut self) -> Option<RpcMsg> {
        self.rx.try_recv().ok()
    }
}

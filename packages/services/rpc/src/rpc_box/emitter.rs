use async_std::channel::bounded;
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::error_handle::ErrorUtils;
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};
use std::{marker::PhantomData, sync::Arc};

use super::{logic::RpcLogic, RpcError};

#[derive(Clone)]
pub struct RpcBoxEmitter<Event, Req, Res> {
    pub(crate) _tmp: PhantomData<(Event, Req, Res)>,
    pub(crate) logic: Arc<RwLock<RpcLogic>>,
}

impl<Event: Serialize, Req: Serialize, Res: 'static + DeserializeOwned + Send + Sync> RpcBoxEmitter<Event, Req, Res> {
    pub fn emit(&self, to_service: u8, dest: RouteRule, event: Event) {
        let buf = bincode::serialize(&event).expect("Should serialize");
        self.logic.write().send_event(to_service, dest, buf);
    }

    pub async fn request(&self, to_service: u8, dest: RouteRule, request: Req, timeout_ms: u64) -> Result<Res, RpcError> {
        let buf = bincode::serialize(&request).expect("Should serialize");
        let (tx, rx) = bounded(1);
        self.logic.write().send_request(
            to_service,
            dest,
            buf,
            Box::new(move |res| {
                let res = match res.map(|buf| bincode::deserialize::<Res>(&buf).map_err(|_| RpcError::DeserializeError)) {
                    Ok(res) => res,
                    Err(e) => Err(e),
                };
                tx.try_send(res).print_error("Should send answer");
            }),
            timeout_ms,
        );
        rx.recv().await.map_err(|_| RpcError::LocalQueueError)?
    }
}

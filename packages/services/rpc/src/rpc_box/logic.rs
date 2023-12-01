use std::{collections::HashMap, sync::Arc};

use crate::RpcError;
use atm0s_sdn_identity::NodeId;
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::{awaker::Awaker, vec_dequeue::VecDeque, Timer};

pub type RpcHandler = Box<dyn FnMut(Result<Vec<u8>, RpcError>) + Send + Sync>;

#[derive(Debug, PartialEq, Eq)]
pub enum RpcLogicEvent {
    Event {
        dest_sevice_id: u8,
        dest: RouteRule,
        data: Vec<u8>,
    },
    Request {
        dest_sevice_id: u8,
        dest: RouteRule,
        req_id: u64,
        data: Vec<u8>,
    },
    Response {
        dest_sevice_id: u8,
        dest: RouteRule,
        req_id: u64,
        res: Result<Vec<u8>, RpcError>,
    },
}

struct RequestSlot {
    timeout_at: u64,
    handler: RpcHandler,
}

pub struct RpcLogic {
    req_id_seed: u64,
    timer: Arc<dyn Timer>,
    outputs: VecDeque<RpcLogicEvent>,
    requests: HashMap<u64, RequestSlot>,
    awaker: Option<Arc<dyn Awaker>>,
}

impl RpcLogic {
    pub fn new(timer: Arc<dyn Timer>) -> Self {
        Self {
            timer,
            req_id_seed: 0,
            outputs: Default::default(),
            requests: Default::default(),
            awaker: None,
        }
    }

    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.awaker = Some(awaker);
    }

    pub fn on_tick(&mut self) {
        //TODO better way to manage timeout
        let now_ms = self.timer.now_ms();
        let mut timeouts = vec![];
        for (req_id, slot) in &self.requests {
            if now_ms >= slot.timeout_at {
                timeouts.push(*req_id);
            }
        }

        for req_id in timeouts {
            if let Some(mut slot) = self.requests.remove(&req_id) {
                (slot.handler)(Err(RpcError::Timeout));
            }
        }
    }

    pub fn send_event(&mut self, dest_sevice_id: u8, dest: RouteRule, event: Vec<u8>) {
        self.outputs.push_back(RpcLogicEvent::Event { dest_sevice_id, dest, data: event });
        self.awake();
    }

    pub fn send_request(&mut self, dest_sevice_id: u8, dest: RouteRule, request: Vec<u8>, handler: RpcHandler, timeout: u64) {
        let req_id = self.req_id_seed;
        self.req_id_seed += 1;
        self.outputs.push_back(RpcLogicEvent::Request {
            dest_sevice_id,
            dest,
            req_id,
            data: request,
        });
        self.requests.insert(
            req_id,
            RequestSlot {
                timeout_at: self.timer.now_ms() + timeout,
                handler,
            },
        );
        self.awake();
    }

    pub fn send_answer(&mut self, dest_sevice_id: u8, to_node: NodeId, req_id: u64, res: Result<Vec<u8>, RpcError>) {
        self.outputs.push_back(RpcLogicEvent::Response {
            dest_sevice_id,
            dest: RouteRule::ToNode(to_node),
            req_id,
            res,
        });
        self.awake();
    }

    pub fn on_answer(&mut self, req_id: u64, res: Result<Vec<u8>, RpcError>) {
        if let Some(mut slot) = self.requests.remove(&req_id) {
            (slot.handler)(res);
        }
    }

    pub fn pop(&mut self) -> Option<RpcLogicEvent> {
        self.outputs.pop_front()
    }

    fn awake(&self) {
        if self.outputs.len() == 1 {
            if let Some(awaker) = &self.awaker {
                awaker.notify();
            } else {
                log::warn!("[RpcLogic] awaker not set");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use atm0s_sdn_router::RouteRule;
    use atm0s_sdn_utils::{
        awaker::{Awaker, MockAwaker},
        MockTimer,
    };
    use parking_lot::Mutex;
    use std::sync::Arc;

    use crate::{rpc_box::logic::RpcLogicEvent, RpcError};

    use super::RpcLogic;

    #[test]
    fn should_send_event() {
        let timer = Arc::new(MockTimer::default());
        let mut logic = RpcLogic::new(timer.clone());
        let awaker = Arc::new(MockAwaker::default());
        logic.set_awaker(awaker.clone());
        logic.send_event(111, RouteRule::ToService(0), vec![1, 2, 3]);
        assert_eq!(
            logic.pop(),
            Some(RpcLogicEvent::Event {
                dest_sevice_id: 111,
                dest: RouteRule::ToService(0),
                data: vec![1, 2, 3]
            })
        );
        assert_eq!(awaker.pop_awake_count(), 1);
    }

    #[test]
    fn should_send_request() {
        let timer = Arc::new(MockTimer::default());
        let mut logic = RpcLogic::new(timer.clone());
        let awaker = Arc::new(MockAwaker::default());
        logic.set_awaker(awaker.clone());
        logic.send_request(111, RouteRule::ToService(0), vec![1, 2, 3], Box::new(|_| {}), 1000);
        assert_eq!(
            logic.pop(),
            Some(RpcLogicEvent::Request {
                dest_sevice_id: 111,
                dest: RouteRule::ToService(0),
                req_id: 0,
                data: vec![1, 2, 3]
            })
        );
        assert_eq!(awaker.pop_awake_count(), 1);
    }

    #[test]
    fn should_send_answer() {
        let timer = Arc::new(MockTimer::default());
        let mut logic = RpcLogic::new(timer.clone());
        let awaker = Arc::new(MockAwaker::default());
        logic.set_awaker(awaker.clone());
        logic.send_answer(111, 100, 1000, Ok(vec![1, 2, 3]));
        assert_eq!(
            logic.pop(),
            Some(RpcLogicEvent::Response {
                dest_sevice_id: 111,
                dest: RouteRule::ToNode(100),
                req_id: 1000,
                res: Ok(vec![1, 2, 3])
            })
        );
        assert_eq!(awaker.pop_awake_count(), 1);
    }

    #[test]
    fn should_timeout_req_without_res() {
        let response = Arc::new(Mutex::new(None));
        let timer = Arc::new(MockTimer::default());
        let mut logic = RpcLogic::new(timer.clone());
        let awaker = Arc::new(MockAwaker::default());
        logic.set_awaker(awaker.clone());
        let response_c = response.clone();
        logic.send_request(
            111,
            RouteRule::ToService(0),
            vec![1, 2, 3],
            Box::new(move |res| {
                response_c.lock().replace(res);
            }),
            1000,
        );
        assert_eq!(
            logic.pop(),
            Some(RpcLogicEvent::Request {
                dest_sevice_id: 111,
                dest: RouteRule::ToService(0),
                req_id: 0,
                data: vec![1, 2, 3]
            })
        );
        assert_eq!(awaker.pop_awake_count(), 1);

        timer.fake(1000);
        logic.on_tick();

        assert_eq!(response.lock().take(), Some(Err(RpcError::Timeout)));
        assert!(logic.requests.is_empty());
    }

    #[test]
    fn should_response() {
        let response = Arc::new(Mutex::new(None));
        let timer = Arc::new(MockTimer::default());
        let mut logic = RpcLogic::new(timer.clone());
        let awaker = Arc::new(MockAwaker::default());
        logic.set_awaker(awaker.clone());
        let response_c = response.clone();
        logic.send_request(
            111,
            RouteRule::ToService(0),
            vec![1, 2, 3],
            Box::new(move |res| {
                response_c.lock().replace(res);
            }),
            1000,
        );
        assert_eq!(
            logic.pop(),
            Some(RpcLogicEvent::Request {
                dest_sevice_id: 111,
                dest: RouteRule::ToService(0),
                req_id: 0,
                data: vec![1, 2, 3]
            })
        );
        assert_eq!(awaker.pop_awake_count(), 1);

        logic.on_answer(0, Ok(vec![4, 5, 6]));

        assert_eq!(response.lock().take(), Some(Ok(vec![4, 5, 6])));
        assert!(logic.requests.is_empty());
    }
}

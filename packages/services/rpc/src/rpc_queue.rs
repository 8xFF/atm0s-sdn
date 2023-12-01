use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::TransportMsg;
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::awaker::Awaker;

use crate::{
    rpc_id_gen::RpcIdGenerate,
    rpc_msg::{RpcError, RpcMsg},
};

pub struct RpcQueue<LD> {
    node_id: NodeId,
    service_id: u8,
    id_gen: RpcIdGenerate,
    reqs: HashMap<u64, (u64, LD)>,
    outs: VecDeque<TransportMsg>,
    awaker: Option<Arc<dyn Awaker>>,
}

impl<LD> RpcQueue<LD> {
    pub fn new(node_id: NodeId, service_id: u8) -> Self {
        Self {
            node_id,
            service_id,
            id_gen: Default::default(),
            reqs: HashMap::new(),
            outs: VecDeque::new(),
            awaker: None,
        }
    }

    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.awaker = Some(awaker);
    }

    pub fn add_request<Req: Into<Vec<u8>>>(&mut self, now_ms: u64, service_id: u8, rule: RouteRule, cmd: &str, param: Req, local_data: LD, timeout_after_ms: u64) {
        let req_id = self.id_gen.generate();
        let rpc = RpcMsg::create_request(self.node_id, self.service_id, cmd, req_id, param);
        self.reqs.insert(req_id, (now_ms + timeout_after_ms, local_data));
        self.outs.push_back(rpc.to_transport_msg(service_id, rule));
        self.awake_if_need();
    }

    pub fn add_event<E: Into<Vec<u8>>>(&mut self, service_id: u8, rule: RouteRule, cmd: &str, event: E) {
        let rpc = RpcMsg::create_event(self.node_id, self.service_id, cmd, event);
        self.outs.push_back(rpc.to_transport_msg(service_id, rule));
        self.awake_if_need();
    }

    pub fn answer_for<Res: Into<Vec<u8>>>(&mut self, req: &RpcMsg, param: Result<Res, RpcError>) {
        let answer = req.answer(self.node_id, self.service_id, param);
        self.outs.push_back(answer.to_transport_msg(req.from_service_id, RouteRule::ToNode(req.from_node_id)));
        self.awake_if_need();
    }

    pub fn take_request(&mut self, req_id: u64) -> Option<LD> {
        self.reqs.remove(&req_id).map(|(_, ld)| ld)
    }

    pub fn pop_timeout(&mut self, now_ms: u64) -> Option<(u64, LD)> {
        let mut timeout = None;
        for (req_id, (timeout_at, _ld)) in &self.reqs {
            if now_ms >= *timeout_at {
                timeout = Some(*req_id);
                break;
            }
        }

        timeout.map(|req_id| {
            let ld = self.reqs.remove(&req_id).expect("Should has").1;
            (req_id, ld)
        })
    }

    pub fn pop_transmit(&mut self) -> Option<TransportMsg> {
        self.outs.pop_front()
    }

    fn awake_if_need(&self) {
        if self.outs.len() == 1 {
            if let Some(awaker) = &self.awaker {
                awaker.notify();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use atm0s_sdn_router::RouteRule;

    use crate::{RpcMsg, RpcMsgParam, RpcQueue};

    #[test]
    fn create_event() {
        let node_id = 1;
        let service_id = 100;
        let to_service_id = 200;
        let mut queue = RpcQueue::<u32>::new(node_id, service_id);

        queue.add_event(to_service_id, RouteRule::ToService(0), "cmd1", vec![1, 2, 3]);
        let transmit = queue.pop_transmit().unwrap();
        let rpc_msg = RpcMsg::try_from(&transmit).unwrap();
        assert_eq!(
            rpc_msg,
            RpcMsg {
                cmd: "cmd1".to_string(),
                from_node_id: node_id,
                from_service_id: service_id,
                param: RpcMsgParam::Event(vec![1, 2, 3]),
            }
        );
    }

    #[test]
    fn create_request() {
        let node_id = 1;
        let service_id = 100;
        let to_service_id = 200;
        let mut queue = RpcQueue::<u32>::new(node_id, service_id);

        queue.add_request(0, to_service_id, RouteRule::ToService(0), "cmd1", vec![1, 2, 3], 12345, 1000);
        let transmit = queue.pop_transmit().unwrap();
        let rpc_msg = RpcMsg::try_from(&transmit).unwrap();
        assert_eq!(
            rpc_msg,
            RpcMsg {
                cmd: "cmd1".to_string(),
                from_node_id: node_id,
                from_service_id: service_id,
                param: RpcMsgParam::Request { req_id: 0, param: vec![1, 2, 3] },
            }
        );

        assert_eq!(queue.take_request(0), Some(12345));
    }

    #[test]
    fn create_request_timeout() {
        let node_id = 1;
        let service_id = 100;
        let to_service_id = 200;
        let mut queue = RpcQueue::<u32>::new(node_id, service_id);

        queue.add_request(0, to_service_id, RouteRule::ToService(0), "cmd1", vec![1, 2, 3], 12345, 1000);
        let transmit = queue.pop_transmit().unwrap();
        let rpc_msg = RpcMsg::try_from(&transmit).unwrap();
        assert_eq!(
            rpc_msg,
            RpcMsg {
                cmd: "cmd1".to_string(),
                from_node_id: node_id,
                from_service_id: service_id,
                param: RpcMsgParam::Request { req_id: 0, param: vec![1, 2, 3] },
            }
        );

        assert_eq!(queue.pop_timeout(999), None);
        assert_eq!(queue.pop_timeout(1000), Some((0, 12345)));
    }

    #[test]
    fn create_answer() {
        let node_id = 1;
        let service_id = 100;
        let from_node_id = 2;
        let from_service_id = 200;
        let mut queue = RpcQueue::<u32>::new(node_id, service_id);

        let incomming_req = RpcMsg {
            cmd: "cmd1".to_string(),
            from_node_id,
            from_service_id,
            param: RpcMsgParam::Request { req_id: 123, param: vec![1, 2, 3] },
        };

        queue.answer_for(&incomming_req, Ok(vec![3, 4, 5]));
        let transmit = queue.pop_transmit().unwrap();
        let rpc_msg = RpcMsg::try_from(&transmit).unwrap();
        assert_eq!(
            rpc_msg,
            RpcMsg {
                cmd: "cmd1".to_string(),
                from_node_id: node_id,
                from_service_id: service_id,
                param: RpcMsgParam::Answer {
                    req_id: 123,
                    param: Ok(vec![3, 4, 5])
                },
            }
        );
    }
}

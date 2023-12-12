use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
use atm0s_sdn_router::RouteRule;
use atm0s_sdn_utils::awaker::Awaker;

use crate::{
    rpc_id_gen::RpcIdGenerate,
    rpc_msg::{RpcError, RpcMsg},
    rpc_reliable::{
        msg::{MSG_ACK, MSG_DATA},
        recv::RpcReliableReceiver,
        send::RpcReliableSender,
    },
};

pub struct RpcQueue<LD> {
    node_id: NodeId,
    service_id: u8,
    id_gen: RpcIdGenerate,
    reqs: HashMap<u64, (u64, LD)>,
    reliable_receiver: RpcReliableReceiver,
    reliable_sender: RpcReliableSender,
    outs: VecDeque<TransportMsg>,
    awaker: Option<Arc<dyn Awaker>>,
    // we should set should_awake to true if outs is empty, then should_awake is set to false when called awake_if_need
    should_awake: bool,
}

impl<LD> RpcQueue<LD> {
    pub fn new(node_id: NodeId, service_id: u8) -> Self {
        Self {
            node_id,
            service_id,
            id_gen: Default::default(),
            reqs: HashMap::new(),
            reliable_receiver: RpcReliableReceiver::new(node_id),
            reliable_sender: RpcReliableSender::new(node_id),
            outs: VecDeque::new(),
            awaker: None,
            should_awake: true,
        }
    }

    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.awaker = Some(awaker);
    }

    pub fn add_request<Req: Into<Vec<u8>>>(&mut self, now_ms: u64, service_id: u8, rule: RouteRule, cmd: &str, param: Req, local_data: LD, timeout_after_ms: u64) {
        log::info!("[RpcQueue] add request {}", cmd);
        let req_id = self.id_gen.generate();
        let rpc = RpcMsg::create_request(self.node_id, self.service_id, cmd, req_id, param);

        let mut header = MsgHeader::build(self.service_id, service_id, rule);
        header.from_node = Some(self.node_id);
        let payload = bincode::serialize(&rpc).expect("Should ok");

        if self.reliable_sender.add_msg(now_ms, header, &payload).is_some() {
            self.reqs.insert(req_id, (now_ms + timeout_after_ms, local_data));
            while let Some(msg) = self.reliable_sender.pop_transport_msg() {
                self.outs.push_back(msg);
            }
        }
        self.awake_if_need();
    }

    pub fn add_event<E: Into<Vec<u8>>>(&mut self, now_ms: u64, service_id: u8, rule: RouteRule, cmd: &str, event: E) {
        log::info!("[RpcQueue] add event {}", cmd);
        let rpc = RpcMsg::create_event(self.node_id, self.service_id, cmd, event);
        let mut header = MsgHeader::build(self.service_id, service_id, rule);
        header.from_node = Some(self.node_id);
        let payload = bincode::serialize(&rpc).expect("Should ok");

        if self.reliable_sender.add_msg(now_ms, header, &payload).is_some() {
            while let Some(msg) = self.reliable_sender.pop_transport_msg() {
                self.outs.push_back(msg);
            }
        }

        self.awake_if_need();
    }

    pub fn answer_for<Res: Into<Vec<u8>>>(&mut self, now_ms: u64, req: &RpcMsg, param: Result<Res, RpcError>) {
        log::info!("[RpcQueue] answer {}", req.cmd);
        let answer = req.answer(self.node_id, self.service_id, param);
        let header = MsgHeader::build(self.service_id, req.from_service_id, RouteRule::ToNode(req.from_node_id)).set_from_node(Some(self.node_id));
        let payload = bincode::serialize(&answer).expect("Should ok");

        if self.reliable_sender.add_msg(now_ms, header, &payload).is_some() {
            while let Some(msg) = self.reliable_sender.pop_transport_msg() {
                self.outs.push_back(msg);
            }
        }
        self.awake_if_need();
    }

    pub fn on_msg(&mut self, now_ms: u64, msg: TransportMsg) -> Option<RpcMsg> {
        match msg.header.meta {
            MSG_ACK => {
                self.reliable_sender.on_ack(msg.header.stream_id);
                None
            }
            MSG_DATA => {
                let res = self.reliable_receiver.on_msg(now_ms, msg);
                while let Some(msg) = self.reliable_receiver.pop_msg() {
                    self.outs.push_back(msg);
                }
                res.map(|(header, payload)| RpcMsg::from_header_payload(&header, &payload)).flatten()
            }
            _ => None,
        }
    }

    pub fn take_request(&mut self, req_id: u64) -> Option<LD> {
        self.reqs.remove(&req_id).map(|(_, ld)| ld)
    }

    pub fn pop_timeout(&mut self, now_ms: u64) -> Option<(u64, LD)> {
        self.reliable_sender.on_tick(now_ms);
        self.reliable_receiver.on_tick(now_ms);
        while let Some(msg) = self.reliable_sender.pop_transport_msg() {
            self.outs.push_back(msg);
        }
        while let Some(msg) = self.reliable_receiver.pop_msg() {
            self.outs.push_back(msg);
        }

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
        if self.outs.len() == 1 {
            self.should_awake = true;
        }
        self.outs.pop_front()
    }

    fn awake_if_need(&mut self) {
        if self.should_awake && !self.outs.is_empty() {
            self.should_awake = false;
            if let Some(awaker) = &self.awaker {
                awaker.notify();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
    use atm0s_sdn_router::RouteRule;
    use atm0s_sdn_utils::awaker::{Awaker, MockAwaker};

    use crate::{
        rpc_reliable::msg::{build_stream_id, MSG_ACK, MSG_DATA},
        RpcMsg, RpcMsgParam, RpcQueue,
    };

    #[test]
    fn create_event() {
        let node_id = 1;
        let service_id = 100;
        let to_service_id = 200;
        let awaker = Arc::new(MockAwaker::default());
        let mut queue = RpcQueue::<u32>::new(node_id, service_id);
        queue.set_awaker(awaker.clone());

        queue.add_event(0, to_service_id, RouteRule::ToService(0), "cmd1", vec![1, 2, 3]);
        assert_eq!(awaker.pop_awake_count(), 1);
        let transmit = queue.pop_transmit().unwrap();
        let rpc_msg = RpcMsg::from_header_payload(&transmit.header, transmit.payload()).unwrap();
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
    fn create_big_event_should_fire_awake() {
        let node_id = 1;
        let service_id = 100;
        let to_service_id = 200;
        let awaker = Arc::new(MockAwaker::default());
        let mut queue = RpcQueue::<u32>::new(node_id, service_id);
        queue.set_awaker(awaker.clone());

        queue.add_event(0, to_service_id, RouteRule::ToService(0), "cmd1", vec![1; 20000]);
        assert_eq!(awaker.pop_awake_count(), 1);

        while let Some(_) = queue.pop_transmit() {}

        queue.add_event(0, to_service_id, RouteRule::ToService(0), "cmd2", vec![2; 20000]);
        assert_eq!(awaker.pop_awake_count(), 1);
    }

    #[test]
    fn create_request() {
        let node_id = 1;
        let service_id = 100;
        let to_service_id = 200;
        let mut queue = RpcQueue::<u32>::new(node_id, service_id);

        queue.add_request(0, to_service_id, RouteRule::ToService(0), "cmd1", vec![1, 2, 3], 12345, 1000);
        let transmit = queue.pop_transmit().unwrap();
        let rpc_msg = RpcMsg::from_header_payload(&transmit.header, transmit.payload()).unwrap();
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
        let rpc_msg = RpcMsg::from_header_payload(&transmit.header, transmit.payload()).unwrap();
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

        queue.answer_for(0, &incomming_req, Ok(vec![3, 4, 5]));
        let transmit = queue.pop_transmit().unwrap();
        let rpc_msg = RpcMsg::from_header_payload(&transmit.header, transmit.payload()).unwrap();
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

    #[test]
    fn queue_handle_incoming_event() {
        let mut queue = RpcQueue::<u32>::new(10, 100);

        let expected_req = RpcMsg {
            cmd: "cmd1".to_string(),
            from_node_id: 11,
            from_service_id: 101,
            param: RpcMsgParam::Request { req_id: 123, param: vec![1, 2, 3] },
        };

        let header = MsgHeader::build(101, 100, RouteRule::Direct)
            .set_from_node(Some(11))
            .set_stream_id(build_stream_id(0, 0, 0))
            .set_meta(MSG_DATA);

        let received_req = queue
            .on_msg(0, TransportMsg::build_raw(header, &bincode::serialize(&expected_req).expect("")))
            .expect("Should finish req");
        assert_eq!(received_req, expected_req);

        let ack_msg = queue.pop_transmit().expect("Should has");
        assert_eq!(ack_msg.header.from_node, Some(10));
        assert_eq!(ack_msg.header.route, RouteRule::ToNode(11));
        assert_eq!(ack_msg.header.from_service_id, 100);
        assert_eq!(ack_msg.header.to_service_id, 101);
        assert_eq!(ack_msg.header.meta, MSG_ACK);
        assert_eq!(ack_msg.payload(), &[]);
    }
}

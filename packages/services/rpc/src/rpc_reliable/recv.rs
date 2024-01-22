use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
use atm0s_sdn_router::RouteRule;

use crate::rpc_reliable::msg::MSG_DATA;

use super::msg::{parse_stream_id, MAX_PART_LEN, MSG_ACK, RESEND_TIMEOUT_MS};

#[derive(Hash, PartialEq, Eq)]
struct MsgKey(NodeId, u16);

struct MsgSlot {
    timeout_at: u64,
    part_count: usize,
    part_received: usize,
    parts: Vec<Option<TransportMsg>>,
}

impl MsgSlot {
    pub fn build(now_ms: u64, stream_id: u32) -> Self {
        let (_msg_id, _index, count_minus_1) = parse_stream_id(stream_id);
        let mut parts = Vec::with_capacity(count_minus_1 as usize + 1);
        for _ in 0..(count_minus_1 as usize + 1) {
            parts.push(None);
        }

        MsgSlot {
            timeout_at: now_ms + RESEND_TIMEOUT_MS * 2,
            part_received: 0,
            part_count: count_minus_1 as usize + 1,
            parts,
        }
    }

    pub fn append_part(&mut self, msg: TransportMsg) -> Option<()> {
        let (_msg_id, index, _count_minus_1) = parse_stream_id(msg.header.stream_id);
        if self.part_received != self.part_count {
            let part_container = &mut self.parts[index as usize];
            if part_container.is_none() {
                part_container.replace(msg);
                self.part_received += 1;
                Some(())
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn is_finish(&self) -> bool {
        self.part_count == self.part_received
    }

    pub fn finalize(&mut self) -> (MsgHeader, Vec<u8>) {
        assert_eq!(self.part_received, self.part_count);
        assert_eq!(self.parts.len(), self.part_count);
        if self.part_count == 1 {
            let msg = self.parts[0].take().expect("Should ok");
            let pay = msg.payload().to_vec();
            (msg.header, pay)
        } else {
            let mut final_buf = Vec::with_capacity(self.part_count * MAX_PART_LEN);
            for part in &self.parts {
                if let Some(msg) = &part {
                    final_buf.extend_from_slice(msg.payload());
                }
            }
            let msg: TransportMsg = self.parts[0].take().expect("Should ok");
            self.parts = vec![];
            (msg.header, final_buf)
        }
    }
}

pub struct RpcReliableReceiver {
    node_id: NodeId,
    queue: HashMap<MsgKey, MsgSlot>,
    output: VecDeque<TransportMsg>,
}

impl RpcReliableReceiver {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            queue: Default::default(),
            output: Default::default(),
        }
    }

    pub fn on_msg(&mut self, now_ms: u64, msg: TransportMsg) -> Option<(MsgHeader, Vec<u8>)> {
        assert_eq!(msg.header.meta, MSG_DATA);
        let from_node = msg.header.from_node?;
        let ack_header = msg
            .header
            .clone()
            .set_from_service_id(msg.header.to_service_id)
            .set_to_service_id(msg.header.from_service_id)
            .set_from_node(Some(self.node_id))
            .set_route(RouteRule::ToNode(from_node))
            .set_meta(MSG_ACK);

        self.output.push_back(TransportMsg::build_raw(ack_header, &[]));

        let (msg_id, _index, count_minus_1) = parse_stream_id(msg.header.stream_id);
        log::info!("[RpcReliableReceiver {}] on msg {}, part {}/{}", self.node_id, msg_id, _index, count_minus_1 + 1);

        if count_minus_1 == 0 {
            //just single msg dont need add to list
            let mut slot = MsgSlot::build(now_ms, msg.header.stream_id);
            slot.append_part(msg)?;
            log::info!("[RpcReliableReceiver {}] on msg single part {} finish", self.node_id, msg_id);
            Some(slot.finalize())
        } else {
            let msg_key = MsgKey(from_node, msg_id);
            let slot = self.queue.entry(msg_key).or_insert_with(|| MsgSlot::build(now_ms, msg.header.stream_id));
            slot.append_part(msg)?;
            if slot.is_finish() {
                let res = slot.finalize();
                self.queue.remove(&MsgKey(from_node, msg_id));
                log::info!("[RpcReliableReceiver {}] on msg {} finish, queue len {}", self.node_id, msg_id, self.queue.len());
                Some(res)
            } else {
                None
            }
        }
    }

    pub fn on_tick(&mut self, now_ms: u64) {
        self.queue.retain(|_, s| s.timeout_at > now_ms);
    }

    pub fn pop_msg(&mut self) -> Option<TransportMsg> {
        self.output.pop_front()
    }
}

#[cfg(test)]
mod test {
    use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
    use atm0s_sdn_router::RouteRule;

    use crate::rpc_reliable::msg::{build_stream_id, MSG_ACK, MSG_DATA, RESEND_TIMEOUT_MS};

    use super::{MsgSlot, RpcReliableReceiver};

    #[test]
    fn slot_single_correct_build() {
        let mut slot = MsgSlot::build(0, build_stream_id(0, 0, 0));
        assert_eq!(slot.timeout_at, RESEND_TIMEOUT_MS * 2);
        assert_eq!(slot.part_count, 1);
        assert_eq!(slot.part_received, 0);

        let header = MsgHeader::build(111, 111, RouteRule::Direct)
            .set_meta(MSG_DATA)
            .set_stream_id(build_stream_id(0, 0, 0))
            .set_from_node(Some(0));
        slot.append_part(TransportMsg::build_raw(header, &[1, 2, 3]));
        assert!(slot.is_finish());
        assert_eq!(slot.part_received, 1);

        let (header, payload) = slot.finalize();
        assert_eq!(header.from_service_id, 111);
        assert_eq!(header.to_service_id, 111);
        assert_eq!(header.meta, MSG_DATA);
        assert_eq!(payload, vec![1, 2, 3]);
    }

    #[test]
    fn slot_multi_correct_build() {
        let mut slot = MsgSlot::build(0, build_stream_id(0, 0, 1));
        assert_eq!(slot.timeout_at, RESEND_TIMEOUT_MS * 2);
        assert_eq!(slot.part_count, 2);
        assert_eq!(slot.part_received, 0);

        let header = MsgHeader::build(111, 111, RouteRule::Direct)
            .set_meta(MSG_DATA)
            .set_stream_id(build_stream_id(0, 0, 1))
            .set_from_node(Some(0));
        assert_eq!(slot.append_part(TransportMsg::build_raw(header.clone(), &[1, 2, 3])), Some(()));
        // add twice should not success
        assert_eq!(slot.append_part(TransportMsg::build_raw(header, &[1, 2, 3])), None);
        assert_eq!(slot.part_received, 1);

        let header: MsgHeader = MsgHeader::build(111, 111, RouteRule::Direct)
            .set_meta(MSG_DATA)
            .set_stream_id(build_stream_id(0, 1, 1))
            .set_from_node(Some(0));
        slot.append_part(TransportMsg::build_raw(header, &[4, 5, 6]));
        assert_eq!(slot.part_received, 2);

        assert!(slot.is_finish());

        let (header, payload) = slot.finalize();
        assert!(slot.parts.is_empty());
        assert_eq!(header.from_service_id, 111);
        assert_eq!(header.to_service_id, 111);
        assert_eq!(header.meta, MSG_DATA);
        assert_eq!(payload, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn receiver_single_slot() {
        let mut receiver = RpcReliableReceiver::new(0);

        let header = MsgHeader::build(111, 112, RouteRule::Direct)
            .set_meta(MSG_DATA)
            .set_stream_id(build_stream_id(0, 0, 0))
            .set_from_node(Some(0));
        let (header, payload) = receiver.on_msg(0, TransportMsg::build_raw(header, &[1, 2, 3])).expect("Should has");
        assert_eq!(header.from_service_id, 111);
        assert_eq!(header.to_service_id, 112);
        assert_eq!(header.meta, MSG_DATA);
        assert_eq!(payload, vec![1, 2, 3]);

        let msg = receiver.pop_msg().expect("Should have");
        assert_eq!(msg.header.from_service_id, 112);
        assert_eq!(msg.header.to_service_id, 111);
        assert_eq!(msg.header.stream_id, build_stream_id(0, 0, 0));
        assert_eq!(msg.header.meta, MSG_ACK);
        assert_eq!(msg.payload(), &[]);
    }

    #[test]
    fn receiver_timeout() {
        let mut receiver = RpcReliableReceiver::new(0);

        let header = MsgHeader::build(111, 111, RouteRule::Direct)
            .set_meta(MSG_DATA)
            .set_stream_id(build_stream_id(0, 0, 1))
            .set_from_node(Some(0));
        receiver.on_msg(0, TransportMsg::build_raw(header, &[1, 2, 3]));

        assert_eq!(receiver.queue.len(), 1);

        receiver.on_tick(RESEND_TIMEOUT_MS * 2);

        assert_eq!(receiver.queue.len(), 0);
    }

    #[test]
    fn receiver_multi_slot() {
        let mut receiver = RpcReliableReceiver::new(0);

        let header = MsgHeader::build(111, 112, RouteRule::Direct)
            .set_meta(MSG_DATA)
            .set_stream_id(build_stream_id(0, 0, 1))
            .set_from_node(Some(0));
        assert_eq!(receiver.on_msg(0, TransportMsg::build_raw(header, &[1, 2, 3])), None);
        let header = MsgHeader::build(111, 112, RouteRule::Direct)
            .set_meta(MSG_DATA)
            .set_stream_id(build_stream_id(0, 1, 1))
            .set_from_node(Some(0));
        let (header, payload) = receiver.on_msg(1, TransportMsg::build_raw(header, &[4, 5, 6])).expect("Should have");
        assert_eq!(header.from_service_id, 111);
        assert_eq!(header.to_service_id, 112);
        assert_eq!(header.meta, MSG_DATA);
        assert_eq!(payload, vec![1, 2, 3, 4, 5, 6]);

        let msg = receiver.pop_msg().expect("Should have");
        assert_eq!(msg.header.from_service_id, 112);
        assert_eq!(msg.header.to_service_id, 111);
        assert_eq!(msg.header.from_node, Some(0));
        assert_eq!(msg.header.stream_id, build_stream_id(0, 0, 1));
        assert_eq!(msg.header.meta, MSG_ACK);
        assert_eq!(msg.payload(), &[]);

        let msg = receiver.pop_msg().expect("Should have");
        assert_eq!(msg.header.from_service_id, 112);
        assert_eq!(msg.header.to_service_id, 111);
        assert_eq!(msg.header.from_node, Some(0));
        assert_eq!(msg.header.stream_id, build_stream_id(0, 1, 1));
        assert_eq!(msg.header.meta, MSG_ACK);
        assert_eq!(msg.payload(), &[]);
    }
}

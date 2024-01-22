use std::collections::{HashMap, VecDeque};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};

use super::msg::{build_stream_id, parse_stream_id, MAX_PART_LEN, MSG_DATA, RESEND_AFTER_MS, RESEND_TIMEOUT_MS};

struct RequestSlot {
    create_at: u64,
    timeout_at: u64,
    acked: usize,
    parts: Vec<Option<TransportMsg>>,
}

/// The `RpcReliableSender` struct is responsible for reliable message sending in Rust.
///

pub struct RpcReliableSender {
    node_id: NodeId,
    msg_id_seed: u16,
    queue: HashMap<u16, RequestSlot>,
    output: VecDeque<TransportMsg>,
}

impl RpcReliableSender {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            msg_id_seed: 0,
            queue: Default::default(),
            output: Default::default(),
        }
    }

    /// Adds a new message to the reliable sender. If payload is bigger than MAX_PART_LEN, it will be split into small chunks
    ///
    /// # Arguments
    ///
    /// * `now_ms` - The current time in milliseconds.
    /// * `header` - An instance of `MsgHeader` representing the message header.
    /// * `payload` - A byte slice containing the message payload.
    ///
    /// # Returns
    ///
    /// Returns `Some(())` if the message was successfully added, or `None` if the `calc_part_count` function returns `None`.
    pub fn add_msg(&mut self, now_ms: u64, header: MsgHeader, payload: &[u8]) -> Option<()> {
        let part_count = calc_part_count(payload.len())?;
        log::info!("[RpcReliableSender {}] create {} part for sending payload {}", self.node_id, part_count, payload.len());
        let mut slot = RequestSlot {
            create_at: now_ms,
            timeout_at: now_ms + RESEND_TIMEOUT_MS,
            acked: 0,
            parts: vec![],
        };

        let msg_id = self.msg_id_seed;
        self.msg_id_seed = self.msg_id_seed.wrapping_add(1);
        for part_index in 0..part_count {
            let header = header.clone().set_stream_id(build_stream_id(msg_id, part_index, part_count - 1)).set_meta(MSG_DATA);
            let msg_pay_start = part_index as usize * MAX_PART_LEN;
            let msg_pay_end = payload.len().min(msg_pay_start + MAX_PART_LEN);
            let msg = TransportMsg::build_raw(header, &payload[msg_pay_start..msg_pay_end]);
            self.output.push_back(msg.clone());
            slot.parts.push(Some(msg));
        }
        self.queue.insert(msg_id, slot);

        Some(())
    }

    /// Handles an acknowledgment for a received message.
    ///
    /// # Arguments
    ///
    /// * `stream_id` - The stream ID of the received acknowledgment.
    pub fn on_ack(&mut self, stream_id: u32) {
        let (msg_id, index, count_minus_1) = parse_stream_id(stream_id);
        log::info!("[RpcReliableSender {}] received ack for msg {}, index {}/{}", self.node_id, msg_id, index, count_minus_1 + 1);
        if let Some(slot) = self.queue.get_mut(&msg_id) {
            if slot.parts.len() == count_minus_1 as usize + 1 {
                if slot.parts.get_mut(index as usize).take().is_some() {
                    slot.acked += 1;
                    if slot.acked == slot.parts.len() {
                        self.queue.remove(&msg_id);
                    }
                }
            }
        }
    }

    /// Handles a tick event for the reliable sender.
    /// Message which is older than RESEND_AFTER_MS but not acked will be resend
    ///
    /// # Arguments
    ///
    /// * `now_ms` - The current time in milliseconds.
    pub fn on_tick(&mut self, now_ms: u64) {
        let mut timout_msg_ids = vec![];
        for (msg_id, slot) in self.queue.iter_mut() {
            if now_ms >= slot.timeout_at {
                log::warn!("[RpcReliableSender {}] msg {} timeout", self.node_id, msg_id);
                timout_msg_ids.push(*msg_id);
            } else if now_ms >= slot.create_at + RESEND_AFTER_MS {
                for part in &slot.parts {
                    if let Some(msg) = part {
                        let (_, index, count) = parse_stream_id(msg.header.stream_id);
                        log::warn!(
                            "[RpcReliableSender {}] resend msg {} part {}/{} after {} ms",
                            self.node_id,
                            msg_id,
                            index,
                            count + 1,
                            now_ms - slot.create_at
                        );
                        self.output.push_back(msg.clone());
                    }
                }
            }
        }

        for msg_id in timout_msg_ids {
            self.queue.remove(&msg_id);
        }
    }

    /// Retrieves the next transport message from the reliable sender's output queue.
    ///
    /// # Returns
    ///
    /// Returns `Some(transport_msg)` if there is a transport message in the output queue, or `None` if the queue is empty.
    pub fn pop_transport_msg(&mut self) -> Option<TransportMsg> {
        self.output.pop_front()
    }
}

/// Calculates the number of parts needed to split a payload based on a maximum part length.
///
/// # Arguments
///
/// * `len` - The length of the payload.
///
/// # Returns
///
/// * `Some(part_count)`: The number of parts needed to split the payload, represented as an Option<u8>.
/// * `None`: If the length of the payload exceeds the maximum allowed.
fn calc_part_count(len: usize) -> Option<u8> {
    if len > MAX_PART_LEN * u8::MAX as usize {
        None
    } else {
        if len % MAX_PART_LEN == 0 {
            Some((len / MAX_PART_LEN) as u8)
        } else {
            Some((len / MAX_PART_LEN) as u8 + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use atm0s_sdn_network::msg::MsgHeader;
    use atm0s_sdn_router::RouteRule;

    use crate::rpc_reliable::{
        msg::build_stream_id,
        send::{calc_part_count, MAX_PART_LEN, RESEND_AFTER_MS, RESEND_TIMEOUT_MS},
    };

    use super::RpcReliableSender;

    #[test]
    fn returns_some_1_when_len_is_1000() {
        let len = 1000;
        let result = calc_part_count(len);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn returns_some_2_when_len_is_2000() {
        let len = 2000;
        let result = calc_part_count(len);
        assert_eq!(result, Some(2));
    }

    // Returns Some(255) when len is 280500
    #[test]
    fn returns_some_255_when_len_is_280500() {
        let len = 280500;
        let result = calc_part_count(len);
        assert_eq!(result, Some(255));
    }

    #[test]
    fn returns_none_when_len_is_max_part_len_times_255_plus_1() {
        let len = MAX_PART_LEN * 255 + 1;
        let result = calc_part_count(len);
        assert_eq!(result, None);
    }

    #[test]
    fn returns_some_1_when_len_is_max_part_len() {
        let len = MAX_PART_LEN;
        let result = calc_part_count(len);
        assert_eq!(result, Some(1));
    }

    #[test]
    fn normal_small_life_cycle() {
        let mut sender = RpcReliableSender::new(0);

        sender.add_msg(0, MsgHeader::build(0, 0, RouteRule::Direct), &[1, 2, 3]);
        assert_eq!(sender.queue.len(), 1);

        let msg = sender.pop_transport_msg().expect("Should has");
        assert_eq!(msg.header.stream_id, build_stream_id(0, 0, 0));
        assert_eq!(msg.payload(), &[1, 2, 3]);

        assert_eq!(sender.pop_transport_msg(), None);

        sender.on_ack(build_stream_id(0, 0, 0));

        assert_eq!(sender.queue.len(), 0);
        sender.on_tick(RESEND_AFTER_MS + 1);
        assert_eq!(sender.pop_transport_msg(), None);
    }

    #[test]
    fn normal_big_life_cycle() {
        let mut sender = RpcReliableSender::new(0);

        sender.add_msg(0, MsgHeader::build(0, 0, RouteRule::Direct), &[1; MAX_PART_LEN + 1]);
        assert_eq!(sender.queue.len(), 1);

        let msg1 = sender.pop_transport_msg().expect("Should has");
        assert_eq!(msg1.header.stream_id, build_stream_id(0, 0, 1));
        assert_eq!(msg1.payload(), &[1; MAX_PART_LEN]);

        let msg2 = sender.pop_transport_msg().expect("Should has");
        assert_eq!(msg2.header.stream_id, build_stream_id(0, 1, 1));
        assert_eq!(msg2.payload(), &[1; 1]);

        assert_eq!(sender.pop_transport_msg(), None);

        sender.on_ack(build_stream_id(0, 0, 1));
        sender.on_ack(build_stream_id(0, 1, 1));

        assert_eq!(sender.queue.len(), 0);
        sender.on_tick(RESEND_AFTER_MS + 1);
        assert_eq!(sender.pop_transport_msg(), None);
    }

    #[test]
    fn resend_after_wait() {
        let mut sender = RpcReliableSender::new(0);

        sender.add_msg(0, MsgHeader::build(0, 0, RouteRule::Direct), &[1, 2, 3]);
        assert_eq!(sender.queue.len(), 1);

        let msg = sender.pop_transport_msg().expect("Should has");
        assert_eq!(msg.header.stream_id, build_stream_id(0, 0, 0));
        assert_eq!(msg.payload(), &[1, 2, 3]);
        assert_eq!(sender.pop_transport_msg(), None);

        assert_eq!(sender.queue.len(), 1);
        sender.on_tick(RESEND_AFTER_MS);

        let msg = sender.pop_transport_msg().expect("Should has");
        assert_eq!(msg.header.stream_id, build_stream_id(0, 0, 0));
        assert_eq!(msg.payload(), &[1, 2, 3]);
        assert_eq!(sender.pop_transport_msg(), None);

        sender.on_tick(RESEND_TIMEOUT_MS);
        assert_eq!(sender.queue.len(), 0);
    }
}

use std::sync::Arc;

use async_std::channel::Sender;
use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::TransportMsg;
use atm0s_sdn_utils::error_handle::ErrorUtils;
use parking_lot::RwLock;

use crate::msg::RpcTransmitEvent;

use super::logic::RpcLogic;

pub struct RpcBoxHandler {
    pub(crate) logic: Arc<RwLock<RpcLogic>>,
    pub(crate) tx: Sender<(NodeId, RpcTransmitEvent)>,
}

impl RpcBoxHandler {
    pub fn on_msg(&mut self, _now_ms: u64, msg: TransportMsg) {
        if let Some(from_node) = msg.header.from_node {
            if let Ok(event) = msg.get_payload_bincode::<RpcTransmitEvent>() {
                if let RpcTransmitEvent::Response(_service_id, req_id, res) = event {
                    self.logic.write().on_answer(req_id, res);
                } else {
                    self.tx.try_send((from_node, event)).print_error("Should send");
                }
            }
        }
    }
}

use std::sync::Arc;

use crate::msg::RpcTransmitEvent;
use async_std::channel::Sender;
use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::msg::{MsgHeader, TransportMsg};
use atm0s_sdn_utils::{awaker::Awaker, error_handle::ErrorUtils};
use parking_lot::RwLock;

use super::{
    handler::RpcBoxHandler,
    logic::{RpcLogic, RpcLogicEvent},
};

pub struct RpcBoxBehaviour {
    pub(crate) service_id: u8,
    pub(crate) node_id: NodeId,
    pub(crate) logic: Arc<RwLock<RpcLogic>>,
    pub(crate) tx: Sender<(NodeId, RpcTransmitEvent)>,
}

impl RpcBoxBehaviour {
    pub fn create_handler(&mut self) -> RpcBoxHandler {
        RpcBoxHandler {
            logic: self.logic.clone(),
            tx: self.tx.clone(),
        }
    }

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

    pub fn set_awaker(&mut self, awaker: Arc<dyn Awaker>) {
        self.logic.write().set_awaker(awaker);
    }

    pub fn on_tick(&mut self, _now_ms: u64) {
        self.logic.write().on_tick();
    }

    pub fn pop_net_out(&mut self) -> Option<TransportMsg> {
        match self.logic.write().pop()? {
            RpcLogicEvent::Event { dest_sevice_id, dest, data } => {
                let mut header = MsgHeader::build_reliable(dest_sevice_id, dest, 0);
                header.from_node = Some(self.node_id);
                let msg = TransportMsg::from_payload_bincode(header, &RpcTransmitEvent::Event(self.service_id, data));
                Some(msg)
            }
            RpcLogicEvent::Request { dest_sevice_id, dest, data, req_id } => {
                let mut header = MsgHeader::build_reliable(dest_sevice_id, dest, 0);
                header.from_node = Some(self.node_id);
                let msg = TransportMsg::from_payload_bincode(header, &RpcTransmitEvent::Request(self.service_id, req_id, data));
                Some(msg)
            }
            RpcLogicEvent::Response { dest_sevice_id, dest, req_id, res } => {
                let mut header = MsgHeader::build_reliable(dest_sevice_id, dest, 0);
                header.from_node = Some(self.node_id);
                let msg = TransportMsg::from_payload_bincode(header, &RpcTransmitEvent::Response(self.service_id, req_id, res));
                Some(msg)
            }
        }
    }
}

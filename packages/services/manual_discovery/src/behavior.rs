use crate::handler::ManualHandler;
use crate::msg::*;
use crate::MANUAL_SERVICE_ID;
use bluesea_identity::{ConnId, NodeAddr, NodeAddrType, NodeId};
use network::behaviour::BehaviorContext;
use network::behaviour::NetworkBehaviorAction;
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::transport::TransportOutgoingLocalUuid;
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use utils::Timer;

const CONNECT_WAIT: [u64; 3] = [5000, 10000, 15000];
const CONNECT_WAIT_MAX: u64 = 30000;

enum OutgoingState {
    New,
    Connecting(u64, Option<ConnId>, usize),
    Connected(u64, ConnId),
    ConnectError(u64, Option<ConnId>, OutgoingConnectionError, usize),
}

struct NodeSlot {
    addr: NodeAddr,
    incoming: Option<ConnId>,
    outgoing: OutgoingState,
}

pub struct ManualBehaviorConf {
    pub node_id: NodeId,
    pub neighbours: Vec<NodeAddr>,
    pub timer: Arc<dyn Timer>,
}

pub struct ManualBehavior<HE> {
    #[allow(unused)]
    node_id: NodeId,
    neighbours: HashMap<NodeId, NodeSlot>,
    timer: Arc<dyn Timer>,
    queue_action: VecDeque<NetworkBehaviorAction<HE>>,
}

impl<HE> ManualBehavior<HE> {
    pub fn new(conf: ManualBehaviorConf) -> Self {
        let mut neighbours = HashMap::new();
        for addr in conf.neighbours {
            if let Some(node_id) = addr.node_id() {
                neighbours.insert(
                    node_id,
                    NodeSlot {
                        addr,
                        incoming: None,
                        outgoing: OutgoingState::New,
                    },
                );
            } else {
                log::warn!("[ManualBehavior] Invalid node addr {:?}", addr)
            }
        }
        Self {
            node_id: conf.node_id,
            neighbours,
            timer: conf.timer,
            queue_action: VecDeque::new(),
        }
    }
}

impl<BE, HE> NetworkBehavior<BE, HE> for ManualBehavior<HE>
where
    BE: From<ManualBehaviorEvent> + TryInto<ManualBehaviorEvent> + Send + Sync + 'static,
    HE: From<ManualHandlerEvent> + TryInto<ManualHandlerEvent> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        MANUAL_SERVICE_ID
    }

    fn on_tick(&mut self, _context: &BehaviorContext, now_ms: u64, _interal_ms: u64) {
        for (node_id, slot) in &mut self.neighbours {
            if slot.incoming.is_none() {
                match &slot.outgoing {
                    OutgoingState::New => {
                        self.queue_action.push_back(NetworkBehaviorAction::ConnectTo(0, *node_id, slot.addr.clone()));
                        slot.outgoing = OutgoingState::Connecting(now_ms, None, 0);
                    }
                    OutgoingState::ConnectError(ts, _conn, _err, count) => {
                        let sleep_ms: &u64 = CONNECT_WAIT.get(*count).unwrap_or(&CONNECT_WAIT_MAX);
                        if ts + *sleep_ms < now_ms {
                            //need reconnect
                            self.queue_action.push_back(NetworkBehaviorAction::ConnectTo(0, *node_id, slot.addr.clone()));
                            slot.outgoing = OutgoingState::Connecting(now_ms, None, count + 1);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn on_awake(&mut self, _ctx: &BehaviorContext, _now_ms: u64) {}

    fn check_incoming_connection(&mut self, _context: &BehaviorContext, _now_ms: u64, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _context: &BehaviorContext, now_ms: u64, node: NodeId, conn_id: ConnId, _local_uuid: TransportOutgoingLocalUuid) -> Result<(), ConnectionRejectReason> {
        if let Some(neighbour) = self.neighbours.get_mut(&node) {
            match neighbour.outgoing {
                OutgoingState::New => {
                    neighbour.outgoing = OutgoingState::Connecting(now_ms, Some(conn_id), 0);
                    Ok(())
                }
                OutgoingState::Connecting(_, _, count) => {
                    neighbour.outgoing = OutgoingState::Connecting(now_ms, Some(conn_id), count);
                    Ok(())
                }
                _ => Err(ConnectionRejectReason::ConnectionLimited),
            }
        } else {
            Ok(())
        }
    }

    fn on_local_msg(&mut self, _context: &BehaviorContext, _now_ms: u64, _msg: network::msg::TransportMsg) {
        panic!("Should not happend");
    }

    fn on_incoming_connection_connected(&mut self, _context: &BehaviorContext, _now_ms: u64, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        let entry = self.neighbours.entry(conn.remote_node_id()).or_insert_with(|| NodeSlot {
            addr: conn.remote_addr(),
            incoming: None,
            outgoing: OutgoingState::New,
        });
        entry.incoming = Some(conn.conn_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        _context: &BehaviorContext,
        _now_ms: u64,
        connection: Arc<dyn ConnectionSender>,
        _local_uuid: TransportOutgoingLocalUuid,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        let entry = self.neighbours.entry(connection.remote_node_id()).or_insert_with(|| NodeSlot {
            addr: connection.remote_addr(),
            incoming: None,
            outgoing: OutgoingState::New,
        });
        entry.outgoing = OutgoingState::Connected(self.timer.now_ms(), connection.conn_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_incoming_connection_disconnected(&mut self, _context: &BehaviorContext, _now_ms: u64, node: NodeId, _conn_id: ConnId) {
        if let Some(slot) = self.neighbours.get_mut(&node) {
            slot.incoming = None;
        }
    }

    fn on_outgoing_connection_disconnected(&mut self, _context: &BehaviorContext, _now_ms: u64, node: NodeId, _conn_id: ConnId) {
        if let Some(slot) = self.neighbours.get_mut(&node) {
            slot.outgoing = OutgoingState::New;
        }
    }

    fn on_outgoing_connection_error(
        &mut self,
        _context: &BehaviorContext,
        _now_ms: u64,
        node_id: NodeId,
        conn_id: Option<ConnId>,
        _local_uuid: TransportOutgoingLocalUuid,
        err: &OutgoingConnectionError,
    ) {
        if let Some(slot) = self.neighbours.get_mut(&node_id) {
            if let OutgoingState::Connecting(_, _, count) = slot.outgoing {
                slot.outgoing = OutgoingState::ConnectError(self.timer.now_ms(), conn_id, err.clone(), count);
            }
        }
    }

    fn on_handler_event(&mut self, _context: &BehaviorContext, _now_ms: u64, _node_id: NodeId, _connection_id: ConnId, _event: BE) {}

    fn on_started(&mut self, _context: &BehaviorContext, _now_ms: u64) {}

    fn on_stopped(&mut self, _context: &BehaviorContext, _now_ms: u64) {}

    fn pop_action(&mut self) -> Option<NetworkBehaviorAction<HE>> {
        self.queue_action.pop_front()
    }
}

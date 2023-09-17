use crate::handler::ManualHandler;
use crate::msg::*;
use crate::MANUAL_SERVICE_ID;
use bluesea_identity::{ConnId, NodeAddr, NodeAddrType, NodeId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::transport::{ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer};
use network::BehaviorAgent;
use std::collections::HashMap;
use std::sync::Arc;
use utils::Timer;

const CONNECT_WAIT: [u64; 3] = [5000, 10000, 15000];
const CONNECT_WAIT_MAX: u64 = 30000;

enum OutgoingState {
    New,
    Connecting(u64, ConnId, usize),
    Connected(u64, ConnId),
    ConnectError(u64, Option<ConnId>, OutgoingConnectionError, usize),
}

struct NodeSlot {
    addr: NodeAddr,
    incoming: Option<ConnId>,
    outgoing: OutgoingState,
}

pub struct ManualBehaviorConf {
    pub neighbours: Vec<NodeAddr>,
    pub timer: Arc<dyn Timer>,
}

pub struct ManualBehavior {
    neighbours: HashMap<NodeId, NodeSlot>,
    timer: Arc<dyn Timer>,
}

impl ManualBehavior {
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
            }
        }
        Self { neighbours, timer: conf.timer }
    }
}

impl<BE, HE, Req, Res> NetworkBehavior<BE, HE, Req, Res> for ManualBehavior
where
    BE: From<ManualBehaviorEvent> + TryInto<ManualBehaviorEvent> + Send + Sync + 'static,
    HE: From<ManualHandlerEvent> + TryInto<ManualHandlerEvent> + Send + Sync + 'static,
    Req: From<ManualReq> + TryInto<ManualReq> + Send + Sync + 'static,
    Res: From<ManualRes> + TryInto<ManualRes> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        MANUAL_SERVICE_ID
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<HE>, ts_ms: u64, _interal_ms: u64) {
        for (node_id, slot) in &mut self.neighbours {
            if slot.incoming.is_none() {
                match &slot.outgoing {
                    OutgoingState::New => match agent.connect_to(*node_id, slot.addr.clone()) {
                        Ok(conn) => {
                            log::info!("[ManualBehavior] connect to {} with addr {} => conn: {}", node_id, slot.addr, conn.conn_id);
                            slot.outgoing = OutgoingState::Connecting(ts_ms, conn.conn_id, 0);
                        }
                        Err(err) => {
                            log::error!("[ManualBehavior] connect to {} with addr {} => error {:?}", node_id, slot.addr, err);
                            slot.outgoing = OutgoingState::ConnectError(ts_ms, None, err, 0);
                        }
                    },
                    OutgoingState::ConnectError(ts, _conn, _err, count) => {
                        let sleep_ms = CONNECT_WAIT.get(*count).unwrap_or(&CONNECT_WAIT_MAX);
                        if ts + *sleep_ms < ts_ms {
                            //need reconnect
                            match agent.connect_to(*node_id, slot.addr.clone()) {
                                Ok(conn) => {
                                    log::info!("[ManualBehavior] reconnect to {} with addr {} => conn: {}", node_id, slot.addr, conn.conn_id);
                                    slot.outgoing = OutgoingState::Connecting(ts_ms, conn.conn_id, count + 1);
                                }
                                Err(err) => {
                                    log::error!("[ManualBehavior] reconnect to {} with addr {} => error {:?}", node_id, slot.addr, err);
                                    slot.outgoing = OutgoingState::ConnectError(ts_ms, None, err, count + 1);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn check_incoming_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(&mut self, _node: NodeId, _conn_id: ConnId) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, conn: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        let entry = self.neighbours.entry(conn.remote_node_id()).or_insert_with(|| NodeSlot {
            addr: conn.remote_addr(),
            incoming: None,
            outgoing: OutgoingState::New,
        });
        entry.incoming = Some(conn.conn_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_outgoing_connection_connected(&mut self, _agent: &BehaviorAgent<HE>, connection: Arc<dyn ConnectionSender>) -> Option<Box<dyn ConnectionHandler<BE, HE>>> {
        let entry = self.neighbours.entry(connection.remote_node_id()).or_insert_with(|| NodeSlot {
            addr: connection.remote_addr(),
            incoming: None,
            outgoing: OutgoingState::New,
        });
        entry.outgoing = OutgoingState::Connected(self.timer.now_ms(), connection.conn_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_incoming_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, connection: Arc<dyn ConnectionSender>) {
        if let Some(slot) = self.neighbours.get_mut(&connection.remote_node_id()) {
            slot.incoming = None;
        }
    }

    fn on_outgoing_connection_disconnected(&mut self, _agent: &BehaviorAgent<HE>, connection: Arc<dyn ConnectionSender>) {
        if let Some(slot) = self.neighbours.get_mut(&connection.remote_node_id()) {
            slot.outgoing = OutgoingState::New;
        }
    }

    fn on_outgoing_connection_error(&mut self, _agent: &BehaviorAgent<HE>, node_id: NodeId, conn_id: ConnId, err: &OutgoingConnectionError) {
        if let Some(slot) = self.neighbours.get_mut(&node_id) {
            match slot.outgoing {
                OutgoingState::Connecting(_, _, count) => slot.outgoing = OutgoingState::ConnectError(self.timer.now_ms(), Some(conn_id), err.clone(), count),
                _ => {}
            }
        }
    }

    fn on_handler_event(&mut self, _agent: &BehaviorAgent<HE>, _node_id: NodeId, _connection_id: ConnId, _event: BE) {}

    fn on_rpc(&mut self, _agent: &BehaviorAgent<HE>, req: Req, res: Box<dyn RpcAnswer<Res>>) -> bool {
        if let Ok(req) = req.try_into() {
            match req {
                ManualReq::AddNeighbors(addrs) => {
                    let mut added_count = 0;
                    for addr in addrs {
                        if let Some(node_id) = addr.node_id() {
                            if self
                                .neighbours
                                .insert(
                                    node_id,
                                    NodeSlot {
                                        addr,
                                        incoming: None,
                                        outgoing: OutgoingState::New,
                                    },
                                )
                                .is_none()
                            {
                                added_count += 1;
                            }
                        }
                    }
                    res.ok(ManualRes::AddNeighborsRes(added_count).into());
                }
                ManualReq::GetNeighbors() => {
                    let mut addrs = vec![];
                    for (_, slot) in &self.neighbours {
                        addrs.push(slot.addr.clone());
                    }
                    res.ok(ManualRes::GetNeighborsRes(addrs).into());
                }
                ManualReq::GetConnections() => {
                    let mut conns = vec![];
                    for (_, slot) in &self.neighbours {
                        if let Some(conn) = slot.incoming {
                            conns.push((conn, slot.addr.clone(), ConnectionState::IncomingConnected));
                        }
                        match slot.outgoing {
                            OutgoingState::Connecting(_, conn, _) => {
                                conns.push((conn, slot.addr.clone(), ConnectionState::OutgoingConnecting));
                            }
                            OutgoingState::Connected(_, conn) => {
                                conns.push((conn, slot.addr.clone(), ConnectionState::OutgoingConnected));
                            }
                            OutgoingState::ConnectError(_, conn, _, _) => {
                                if let Some(conn_id) = conn {
                                    conns.push((conn_id, slot.addr.clone(), ConnectionState::OutgoingError));
                                }
                            }
                            _ => {}
                        }
                    }
                    res.ok(ManualRes::GetConnectionsRes(conns).into());
                }
            }
        }
        true
    }
}

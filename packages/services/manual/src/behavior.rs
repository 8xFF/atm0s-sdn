use crate::handler::ManualHandler;
use crate::msg::*;
use crate::MANUAL_SERVICE_ID;
use bluesea_identity::{PeerAddr, PeerAddrType, PeerId};
use network::behaviour::{ConnectionHandler, NetworkBehavior};
use network::transport::{
    ConnectionRejectReason, ConnectionSender, OutgoingConnectionError, RpcAnswer,
    TransportPendingOutgoing,
};
use network::BehaviorAgent;
use std::collections::HashMap;
use std::sync::Arc;
use utils::Timer;

const CONNECT_WAIT: [u64; 3] = [5000, 10000, 15000];
const CONNECT_WAIT_MAX: u64 = 30000;

enum OutgoingState {
    New,
    Connecting(u64, u32, usize),
    Connected(u64, u32),
    ConnectError(u64, Option<u32>, OutgoingConnectionError, usize),
}

struct PeerSlot {
    addr: PeerAddr,
    incoming: Option<u32>,
    outgoing: OutgoingState,
}

pub struct ManualBehaviorConf {
    neighbours: Vec<PeerAddr>,
    timer: Arc<dyn Timer>,
}

pub struct ManualBehavior {
    neighbours: HashMap<PeerId, PeerSlot>,
    timer: Arc<dyn Timer>,
}

impl ManualBehavior {
    pub fn new(conf: ManualBehaviorConf) -> Self {
        let mut neighbours = HashMap::new();
        for addr in conf.neighbours {
            if let Some(node_id) = addr.peer_id() {
                neighbours.insert(
                    node_id,
                    PeerSlot {
                        addr,
                        incoming: None,
                        outgoing: OutgoingState::New,
                    },
                );
            }
        }
        Self {
            neighbours,
            timer: conf.timer,
        }
    }
}

impl<BE, HE, MSG, Req, Res> NetworkBehavior<BE, HE, MSG, Req, Res> for ManualBehavior
where
    BE: From<ManualBehaviorEvent> + TryInto<ManualBehaviorEvent> + Send + Sync + 'static,
    HE: From<ManualHandlerEvent> + TryInto<ManualHandlerEvent> + Send + Sync + 'static,
    MSG: From<ManualMsg> + TryInto<ManualMsg> + Send + Sync + 'static,
    Req: From<ManualReq> + TryInto<ManualReq> + Send + Sync + 'static,
    Res: From<ManualRes> + TryInto<ManualRes> + Send + Sync + 'static,
{
    fn service_id(&self) -> u8 {
        MANUAL_SERVICE_ID
    }

    fn on_tick(&mut self, agent: &BehaviorAgent<HE, MSG>, ts_ms: u64, interal_ms: u64) {
        for (peer_id, slot) in &mut self.neighbours {
            if slot.incoming.is_none() {
                match &slot.outgoing {
                    OutgoingState::New => match agent.connect_to(*peer_id, slot.addr.clone()) {
                        Ok(conn) => {
                            log::info!(
                                "[ManualBehavior] connect to {} with addr {} => conn: {}",
                                peer_id,
                                slot.addr,
                                conn.connection_id
                            );
                            slot.outgoing = OutgoingState::Connecting(ts_ms, conn.connection_id, 0);
                        }
                        Err(err) => {
                            log::error!(
                                "[ManualBehavior] connect to {} with addr {} => error {:?}",
                                peer_id,
                                slot.addr,
                                err
                            );
                            slot.outgoing = OutgoingState::ConnectError(ts_ms, None, err, 0);
                        }
                    },
                    OutgoingState::ConnectError(ts, conn, err, count) => {
                        let sleep_ms = CONNECT_WAIT.get(*count).unwrap_or(&CONNECT_WAIT_MAX);
                        if ts + *sleep_ms < ts_ms {
                            //need reconnect
                            match agent.connect_to(*peer_id, slot.addr.clone()) {
                                Ok(conn) => {
                                    log::info!(
                                        "[ManualBehavior] reconnect to {} with addr {} => conn: {}",
                                        peer_id,
                                        slot.addr,
                                        conn.connection_id
                                    );
                                    slot.outgoing = OutgoingState::Connecting(
                                        ts_ms,
                                        conn.connection_id,
                                        count + 1,
                                    );
                                }
                                Err(err) => {
                                    log::error!("[ManualBehavior] reconnect to {} with addr {} => error {:?}", peer_id, slot.addr, err);
                                    slot.outgoing =
                                        OutgoingState::ConnectError(ts_ms, None, err, count + 1);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn check_incoming_connection(
        &mut self,
        peer: PeerId,
        conn_id: u32,
    ) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn check_outgoing_connection(
        &mut self,
        peer: PeerId,
        conn_id: u32,
    ) -> Result<(), ConnectionRejectReason> {
        Ok(())
    }

    fn on_incoming_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>> {
        let entry = self
            .neighbours
            .entry(connection.remote_peer_id())
            .or_insert_with(|| PeerSlot {
                addr: connection.remote_addr(),
                incoming: None,
                outgoing: OutgoingState::New,
            });
        entry.incoming = Some(connection.connection_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_outgoing_connection_connected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) -> Option<Box<dyn ConnectionHandler<BE, HE, MSG>>> {
        let entry = self
            .neighbours
            .entry(connection.remote_peer_id())
            .or_insert_with(|| PeerSlot {
                addr: connection.remote_addr(),
                incoming: None,
                outgoing: OutgoingState::New,
            });
        entry.outgoing = OutgoingState::Connected(self.timer.now_ms(), connection.connection_id());
        Some(Box::new(ManualHandler {}))
    }

    fn on_incoming_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) {
        if let Some(slot) = self.neighbours.get_mut(&connection.remote_peer_id()) {
            slot.incoming = None;
        }
    }

    fn on_outgoing_connection_disconnected(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        connection: Arc<dyn ConnectionSender<MSG>>,
    ) {
        if let Some(slot) = self.neighbours.get_mut(&connection.remote_peer_id()) {
            slot.outgoing = OutgoingState::New;
        }
    }

    fn on_outgoing_connection_error(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        peer_id: PeerId,
        connection_id: u32,
        err: &OutgoingConnectionError,
    ) {
        if let Some(slot) = self.neighbours.get_mut(&peer_id) {
            match slot.outgoing {
                OutgoingState::Connecting(_, _, count) => {
                    slot.outgoing = OutgoingState::ConnectError(
                        self.timer.now_ms(),
                        Some(connection_id),
                        err.clone(),
                        count,
                    )
                }
                _ => {}
            }
        }
    }

    fn on_handler_event(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        peer_id: PeerId,
        connection_id: u32,
        event: BE,
    ) {
    }

    fn on_rpc(
        &mut self,
        agent: &BehaviorAgent<HE, MSG>,
        req: Req,
        res: Box<dyn RpcAnswer<Res>>,
    ) -> bool {
        if let Ok(req) = req.try_into() {
            match req {
                ManualReq::AddNeighbors(addrs) => {
                    let mut added_count = 0;
                    for addr in addrs {
                        if let Some(node_id) = addr.peer_id() {
                            if self
                                .neighbours
                                .insert(
                                    node_id,
                                    PeerSlot {
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
                            conns.push((
                                conn,
                                slot.addr.clone(),
                                ConnectionState::IncomingConnected,
                            ));
                        }
                        match slot.outgoing {
                            OutgoingState::Connecting(_, conn, _) => {
                                conns.push((
                                    conn,
                                    slot.addr.clone(),
                                    ConnectionState::OutgoingConnecting,
                                ));
                            }
                            OutgoingState::Connected(_, conn) => {
                                conns.push((
                                    conn,
                                    slot.addr.clone(),
                                    ConnectionState::OutgoingConnected,
                                ));
                            }
                            OutgoingState::ConnectError(_, conn, _, _) => {
                                if let Some(conn_id) = conn {
                                    conns.push((
                                        conn_id,
                                        slot.addr.clone(),
                                        ConnectionState::OutgoingError,
                                    ));
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

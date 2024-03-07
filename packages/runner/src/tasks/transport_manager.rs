use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    time::Instant,
};

use sans_io_runtime::{bus::BusEvent, Task, TaskInput, TaskOutput};

use super::{
    connection::ConnId,
    events::{ConnectionMessage, TransportEvent, TransportWorkerEvent},
    time::TimeTicker,
};

pub type ChannelIn = ();
pub type ChannelOut = ();

const TICK_MS: u64 = 100;
const RESEND_CONNECT_MS: u64 = 500;
const CONNECT_TIMEOUT_MS: u64 = 10000;

#[derive(Debug, Clone)]
pub enum EventIn {
    ConnectTo(SocketAddr),
    UnhandleNetData(SocketAddr, ConnectionMessage),
    Disconnected(ConnId),
}

pub enum EventOut {
    Transport(TransportEvent),
    Worker(TransportWorkerEvent),
    PassthroughConnectionData(ConnId, ConnectionMessage),
}

fn build_out(out: EventOut) -> TaskOutput<'static, ChannelIn, ChannelOut, EventOut> {
    TaskOutput::Bus(BusEvent::ChannelPublish((), true, out))
}

struct OutgoingSlot {
    created_at: Instant,
    last_sent: Instant,
}

pub struct TransportManagerTask {
    node_id: u32,
    password: String,
    conn_id_seed: u64,
    seeds: Vec<SocketAddr>,
    outgoing_connections: HashMap<SocketAddr, OutgoingSlot>,
    connections: HashMap<SocketAddr, ConnId>,
    connections_reverse: HashMap<ConnId, SocketAddr>,
    ticker: TimeTicker,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
}

impl TransportManagerTask {
    pub fn build(node_id: u32, password: String, seeds: Vec<SocketAddr>) -> Self {
        log::info!("Create TransportManagerTask with seeds: {:?}", seeds);
        Self {
            node_id,
            password,
            conn_id_seed: 0,
            seeds,
            outgoing_connections: HashMap::new(),
            connections: HashMap::new(),
            connections_reverse: HashMap::new(),
            ticker: TimeTicker::build(TICK_MS),
            queue: VecDeque::from([TaskOutput::Bus(BusEvent::ChannelSubscribe(()))]),
        }
    }

    fn generate_conn_id(&mut self) -> ConnId {
        self.conn_id_seed += 1;
        ConnId(self.conn_id_seed)
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for TransportManagerTask {
    const TYPE: u16 = 1;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        if let Some(e) = self.queue.pop_front() {
            return Some(e);
        }

        if !self.ticker.tick(now) {
            return None;
        }

        for seed in &self.seeds {
            if !self.outgoing_connections.contains_key(seed) && !self.connections.contains_key(seed) {
                let seed = seed.clone();
                log::info!("TransportManagerTask request connect to {seed}");
                self.outgoing_connections.insert(seed.clone(), OutgoingSlot { created_at: now, last_sent: now });
                self.queue.push_back(build_out(EventOut::Worker(TransportWorkerEvent::SendTo(
                    seed,
                    ConnectionMessage::ConnectRequest {
                        node_id: self.node_id,
                        meta: "this is meta".to_string(),
                        password: self.password.clone(),
                    },
                ))));
            }
        }

        let mut timeout_outgoings = vec![];
        for (seed, slot) in &mut self.outgoing_connections {
            let wait_ms = now.duration_since(slot.last_sent).as_millis() as u64;
            if wait_ms >= CONNECT_TIMEOUT_MS {
                log::info!("TransportManagerTask connect to {seed} timeout");
                timeout_outgoings.push(seed.clone());
            } else if wait_ms >= RESEND_CONNECT_MS {
                log::info!("TransportManagerTask resend connect to {seed}");
                slot.last_sent = now;
                self.queue.push_back(build_out(EventOut::Worker(TransportWorkerEvent::SendTo(
                    seed.clone(),
                    ConnectionMessage::ConnectRequest {
                        node_id: self.node_id,
                        meta: "this is meta".to_string(),
                        password: self.password.clone(),
                    },
                ))));
            }
        }

        for seed in timeout_outgoings {
            self.outgoing_connections.remove(&seed);
        }

        self.queue.pop_front()
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        let event = if let TaskInput::Bus(_, event) = input {
            event
        } else {
            panic!("Invalid input type for TransportManager")
        };

        match event {
            EventIn::ConnectTo(addr) => {
                self.seeds.push(addr);
                None
            }
            EventIn::Disconnected(conn) => {
                let remote = self.connections_reverse.remove(&conn)?;
                self.connections.remove(&remote);
                self.queue.push_back(build_out(EventOut::Worker(TransportWorkerEvent::UnPinConnection(conn))));
                Some(build_out(EventOut::Transport(TransportEvent::Disconnected(conn))))
            }
            EventIn::UnhandleNetData(remote, event) => match event {
                ConnectionMessage::Ping(..) => None,
                ConnectionMessage::Pong(..) => None,
                ConnectionMessage::ConnectRequest { node_id, meta, password } => {
                    let res = if password.eq(&self.password) {
                        if self.connections.contains_key(&remote) {
                            log::warn!("TransportManagerTask connect from {remote} but already connected");
                            Err("Already connected".to_string())
                        } else {
                            let conn_id = self.generate_conn_id();
                            log::info!("TransportManagerTask connect from {remote} success => create Connection {:?}", conn_id);
                            self.connections.insert(remote, conn_id);
                            self.connections_reverse.insert(conn_id, remote);
                            self.queue.push_back(build_out(EventOut::Transport(TransportEvent::Connected(conn_id))));
                            self.queue.push_back(build_out(EventOut::Worker(TransportWorkerEvent::PinConnection(conn_id, remote))));
                            Ok(self.node_id)
                        }
                    } else {
                        log::warn!("TransportManagerTask connect from {remote} with wrong password, ({password}) vs ({})", self.password);
                        Err("Wrong password".to_string())
                    };
                    Some(build_out(EventOut::Worker(TransportWorkerEvent::SendTo(remote, ConnectionMessage::ConnectResponse(res)))))
                }
                ConnectionMessage::ConnectResponse(res) => match res {
                    Ok(node_id) => {
                        self.outgoing_connections.remove(&remote);
                        if !self.connections.contains_key(&remote) {
                            let conn_id = self.generate_conn_id();
                            log::info!("TransportManagerTask connect to node {node_id} at {remote} success => create Connection {:?}", conn_id);
                            self.connections.insert(remote, conn_id);
                            self.connections_reverse.insert(conn_id, remote);
                            self.queue.push_back(build_out(EventOut::Worker(TransportWorkerEvent::PinConnection(conn_id, remote))));
                            Some(build_out(EventOut::Transport(TransportEvent::Connected(conn_id))))
                        } else {
                            log::warn!("TransportManagerTask response from {remote} but already connected");
                            None
                        }
                    }
                    Err(err) => {
                        self.outgoing_connections.remove(&remote);
                        log::warn!("TransportManagerTask connect to {remote} failed: {err}");
                        //TODO get correct conn_id
                        Some(build_out(EventOut::Transport(TransportEvent::OutgoingError(ConnId(0), err))))
                    }
                },
                ConnectionMessage::Data(data) => {
                    let conn = self.connections.get(&remote)?;
                    Some(build_out(EventOut::PassthroughConnectionData(*conn, ConnectionMessage::Data(data))))
                }
            },
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }
}

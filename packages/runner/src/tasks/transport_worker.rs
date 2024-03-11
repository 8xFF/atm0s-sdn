use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Instant,
};

use sans_io_runtime::{bus::BusEvent, Buffer, NetIncoming, NetOutgoing, Task, TaskInput, TaskOutput};

use super::{
    connection::ConnId,
    events::{ConnectionMessage, TransportWorkerEvent},
};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ChannelIn {
    Broadcast,
    Worker(u16),
}

pub type ChannelOut = ();

pub type EventIn = TransportWorkerEvent;

pub enum EventOut {
    Connection(ConnId, ConnectionMessage),
    UnhandleData(SocketAddr, ConnectionMessage),
}

pub struct TransportWorkerTask {
    port: u16,
    backend_slot: usize,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
    connections: HashMap<SocketAddr, ConnId>,
    connections_reverse: HashMap<ConnId, SocketAddr>,
}

impl TransportWorkerTask {
    pub fn build(worker: u16, port: u16) -> Self {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port));
        Self {
            port,
            backend_slot: 0,
            queue: VecDeque::from([
                TaskOutput::Net(NetOutgoing::UdpListen { addr, reuse: true }),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Broadcast)),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Worker(worker))),
            ]),
            connections: HashMap::new(),
            connections_reverse: HashMap::new(),
        }
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for TransportWorkerTask {
    /// The type identifier for the task.
    const TYPE: u16 = 2;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        match input {
            TaskInput::Net(NetIncoming::UdpListenResult { bind, result }) => {
                let res = result.expect("Should bind to port");
                log::info!("TransportWorkerTask bind to port {} with result: {:?}", self.port, res);
                self.backend_slot = res.1;
                None
            }
            TaskInput::Net(NetIncoming::UdpPacket { slot, from, data }) => {
                let msg: ConnectionMessage = bincode::deserialize(data).ok()?;
                if let Some(conn) = self.connections.get(&from) {
                    log::debug!("TransportWorkerTask receive data from: {}, conn: {:?}, {:?}", from, conn, msg);
                    Some(TaskOutput::Bus(BusEvent::ChannelPublish((), false, EventOut::Connection(*conn, msg))))
                } else {
                    log::debug!("TransportWorkerTask receive data from unknown source: {}", from);
                    Some(TaskOutput::Bus(BusEvent::ChannelPublish((), false, EventOut::UnhandleData(from, msg))))
                }
            }
            TaskInput::Bus(_, event) => match event {
                TransportWorkerEvent::PinConnection(conn, remote) => {
                    log::info!("TransportWorkerTask pin connection: {:?} -> {}", conn, remote);
                    self.connections.insert(remote, conn);
                    self.connections_reverse.insert(conn, remote);
                    None
                }
                TransportWorkerEvent::UnPinConnection(conn) => {
                    if let Some(remote) = self.connections_reverse.remove(&conn) {
                        log::info!("TransportWorkerTask unpin connection: {:?} -> {}", conn, remote);
                        self.connections.remove(&remote);
                    }
                    None
                }
                TransportWorkerEvent::SendConn(conn, data) => {
                    let remote = self.connections_reverse.get(&conn)?;
                    log::debug!("TransportWorkerTask send data to: {:?} -> {}, {:?}", conn, remote, data);
                    let buf = bincode::serialize(&data).ok()?;
                    Some(TaskOutput::Net(NetOutgoing::UdpPacket {
                        slot: self.backend_slot,
                        to: *remote,
                        data: Buffer::Vec(buf),
                    }))
                }
                TransportWorkerEvent::SendTo(remote, data) => {
                    let buf = bincode::serialize(&data).ok()?;
                    log::debug!("TransportWorkerTask send data to: {}, {:?}", remote, data);
                    Some(TaskOutput::Net(NetOutgoing::UdpPacket {
                        slot: self.backend_slot,
                        to: remote,
                        data: Buffer::Vec(buf),
                    }))
                }
            },
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }

    fn shutdown<'a>(
            &mut self,
            now: Instant,
        ) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        None
    }
}

use std::{collections::VecDeque, time::Instant};

use sans_io_runtime::{bus::BusEvent as RuntimeBusEvent, Task, TaskInput, TaskOutput};

use super::{
    events::{BusEvent, ConnectionMessage, ServiceId},
    time::{TimePivot, TimeTicker},
};

const PING_MS: u64 = 1000;
const CONNECTION_TIMEOUT_MS: u64 = 10000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnId(pub u64);

#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub rtt: u8,
}

pub type ChannelIn = ConnId;

pub type ChannelOut = ();

#[derive(Debug, Clone)]
pub enum EventIn {
    Net(ConnectionMessage),
    Bus(ServiceId, BusEvent<Vec<u8>>),
}

pub enum EventOut {
    Disconnected(ConnId),
    Net(ConnId, ConnectionMessage),
    ToBehaviorBus(ConnId, ServiceId, String),
    ToHandleBus(ConnId, ConnId, ServiceId, Vec<u8>),
}

fn build_out<'a>(event: EventOut) -> TaskOutput<'a, ChannelIn, ChannelOut, EventOut> {
    TaskOutput::Bus(RuntimeBusEvent::ChannelPublish((), true, event))
}

pub struct SpawnCfg {
    pub node_id: u32,
    pub conn_id: ConnId,
}

pub struct ConnectionTask {
    node_id: u32,
    conn_id: ConnId,
    timer: TimePivot,
    ping_id: u32,
    ping_ticker: TimeTicker,
    last_data_ms: u64,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
    disconnecting: bool,
}

impl ConnectionTask {
    pub fn build(cfg: SpawnCfg) -> Self {
        Self {
            node_id: cfg.node_id,
            conn_id: cfg.conn_id,
            timer: TimePivot::build(),
            ping_id: 0,
            ping_ticker: TimeTicker::build(PING_MS),
            queue: VecDeque::from([TaskOutput::Bus(RuntimeBusEvent::ChannelSubscribe(cfg.conn_id))]),
            last_data_ms: 0,
            disconnecting: false,
        }
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for ConnectionTask {
    /// The type identifier for the task.
    const TYPE: u16 = 3;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        if self.last_data_ms == 0 {
            self.last_data_ms = self.timer.timestamp_ms(now);
        }

        if let Some(e) = self.queue.pop_front() {
            return Some(e);
        }

        if self.ping_ticker.tick(now) {
            if self.disconnecting {
                self.queue.push_back(build_out(EventOut::Net(self.conn_id, ConnectionMessage::DisconnectRequest("Shutdown".to_string()))));
            }

            self.ping_id += 1;
            log::debug!("Send ping {}", self.ping_id);
            if self.last_data_ms + CONNECTION_TIMEOUT_MS < self.timer.timestamp_ms(now) {
                log::warn!("Connection timeout for {:?}", self.conn_id);
                Some(build_out(EventOut::Disconnected(self.conn_id)))
            } else {
                Some(build_out(EventOut::Net(self.conn_id, ConnectionMessage::Ping(self.timer.timestamp_us(now), self.ping_id))))
            }
        } else {
            None
        }
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        let event = if let TaskInput::Bus(_, event) = input {
            event
        } else {
            panic!("Invalid input type for ConnectionTask")
        };

        match event {
            EventIn::Bus(service, event) => {
                todo!()
            }
            EventIn::Net(event) => match event {
                ConnectionMessage::Ping(timestamp, ping_id) => {
                    self.last_data_ms = self.timer.timestamp_ms(now);
                    log::debug!("Received ping from remote: {:?}, id {}", timestamp, ping_id);
                    Some(build_out(EventOut::Net(self.conn_id, ConnectionMessage::Pong(timestamp, ping_id))))
                }
                ConnectionMessage::Pong(timestamp, ping_id) => {
                    self.last_data_ms = self.timer.timestamp_ms(now);
                    let delta = self.timer.timestamp_us(now) - timestamp;
                    log::info!("Received pong for ping {} from remote after {} us, {} ms", ping_id, delta, delta / 1000);
                    None
                }
                ConnectionMessage::DisconnectRequest(reason) => {
                    log::debug!("Received disconnect request from remote: {:?}", reason);
                    self.queue.push_back(TaskOutput::Destroy);
                    Some(build_out(EventOut::Disconnected(self.conn_id)))
                }
                ConnectionMessage::DisconnectResponse => {
                    log::debug!("Received disconnect response from remote");
                    self.queue.push_back(TaskOutput::Destroy);
                    Some(build_out(EventOut::Disconnected(self.conn_id)))
                }
                ConnectionMessage::Data(data) => {
                    log::debug!("Received data from remote: {:?}", data);
                    None
                }
                ConnectionMessage::ConnectRequest { node_id, meta, password } => {
                    log::debug!("Received connect request from remote: {:?} {:?} {:?}", node_id, meta, password);
                    Some(build_out(EventOut::Net(self.conn_id, ConnectionMessage::ConnectResponse(Ok(self.node_id)))))
                }
                ConnectionMessage::ConnectResponse(response) => {
                    log::debug!("Received connect response from remote: {:?}", response);
                    None
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
        self.disconnecting = true;
        Some(build_out(EventOut::Net(self.conn_id, ConnectionMessage::DisconnectRequest("Shutdown".to_string()))))
    }
}

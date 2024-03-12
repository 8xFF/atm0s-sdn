use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Instant,
};

use atm0s_sdn_network::event::DataEvent;
use atm0s_sdn_router::shadow::{ShadowRouter, ShadowRouterDelta};
use sans_io_runtime::{bus::BusEvent, Buffer, NetIncoming, NetOutgoing, Task, TaskInput, TaskOutput};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ChannelIn {
    Broadcast,
    Worker(u16),
}

pub type ChannelOut = ();

#[derive(Debug, Clone)]
pub enum EventIn {
    Data(SocketAddr, DataEvent),
    RouterRule(ShadowRouterDelta<SocketAddr>),
}

#[derive(Debug, Clone)]
pub enum EventOut {
    Data(SocketAddr, DataEvent),
}

pub struct DataPlaneTask {
    worker: u16,
    port: u16,
    backend_slot: usize,
    router: ShadowRouter<SocketAddr>,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
}

impl DataPlaneTask {
    pub fn build(worker: u16, port: u16) -> Self {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), port));
        Self {
            worker,
            port,
            backend_slot: 0,
            router: ShadowRouter::new(),
            queue: VecDeque::from([
                TaskOutput::Net(NetOutgoing::UdpListen { addr, reuse: true }),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Broadcast)),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Worker(worker))),
            ]),
        }
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for DataPlaneTask {
    /// The type identifier for the task.
    const TYPE: u16 = 1;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        match input {
            TaskInput::Net(net) => match net {
                NetIncoming::UdpListenResult { bind, result } => {
                    let res = result.expect("Should to bind UDP socket");
                    self.backend_slot = res.1;
                    log::info!("Data plane task bound to {}", res.0);
                    None
                }
                NetIncoming::UdpPacket { slot, from, data } => {
                    let data = DataEvent::try_from(data).ok()?;
                    log::trace!("Received from remote {} {:?}", from, data);
                    Some(TaskOutput::Bus(BusEvent::ChannelPublish((), true, EventOut::Data(from, data))))
                }
            },
            TaskInput::Bus(_, EventIn::Data(remote, msg)) => {
                log::trace!("Sending to remote {:?}", msg);
                Some(TaskOutput::Net(NetOutgoing::UdpPacket {
                    slot: self.backend_slot,
                    to: remote,
                    data: Buffer::Vec(msg.into()),
                }))
            }
            TaskInput::Bus(_, EventIn::RouterRule(rule)) => {
                log::info!("On apply router rule {:?}", rule);
                self.router.apply_delta(rule);
                None
            }
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.push_back(TaskOutput::Net(NetOutgoing::UdpUnlisten { slot: self.backend_slot }));
        self.queue.push_back(TaskOutput::Bus(BusEvent::ChannelUnsubscribe(ChannelIn::Broadcast)));
        self.queue.push_back(TaskOutput::Bus(BusEvent::ChannelUnsubscribe(ChannelIn::Worker(self.worker))));
        None
    }
}

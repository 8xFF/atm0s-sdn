use std::{
    collections::VecDeque,
    fmt::Debug,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
    time::Instant,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::{
    base::{GenericBuffer, ServiceBuilder},
    data_plane::{DataPlane, Input as DataPlaneInput, NetInput, NetOutput, Output as DataPlaneOutput},
    features::{FeaturesControl, FeaturesEvent},
    ExtOut, LogicControl, LogicEvent,
};
use sans_io_runtime::{bus::BusEvent, Buffer, NetIncoming, NetOutgoing, Task, TaskInput, TaskOutput};

use crate::time::TimePivot;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ChannelIn {
    Broadcast,
    Worker(u16),
}

pub type ChannelOut = ();

pub type EventIn<TW> = LogicEvent<TW>;
pub type EventOut<TC> = LogicControl<TC>;

pub struct DataPlaneCfg<SC, SE, TC, TW> {
    pub worker: u16,
    pub node_id: NodeId,
    pub port: u16,
    pub services: Vec<Arc<dyn ServiceBuilder<FeaturesControl, FeaturesEvent, SC, SE, TC, TW>>>,
    #[cfg(feature = "vpn")]
    pub vpn_tun_fd: Option<sans_io_runtime::backend::tun::TunFd>,
}

pub struct DataPlaneTask<SC, SE, TC, TW> {
    #[allow(unused)]
    node_id: NodeId,
    worker: u16,
    data_plane: DataPlane<SC, SE, TC, TW>,
    backend_udp_slot: usize,
    timer: TimePivot,
    #[cfg(feature = "vpn")]
    backend_tun_slot: usize,
    queue: VecDeque<TaskOutput<'static, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>>,
}

impl<SC, SE, TC, TW> DataPlaneTask<SC, SE, TC, TW> {
    pub fn build(cfg: DataPlaneCfg<SC, SE, TC, TW>) -> Self {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), cfg.port));
        let mut queue = VecDeque::from([
            TaskOutput::Net(NetOutgoing::UdpListen { addr, reuse: true }),
            TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Broadcast)),
            TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Worker(cfg.worker))),
        ]);
        #[cfg(feature = "vpn")]
        if let Some(fd) = cfg.vpn_tun_fd {
            queue.push_back(TaskOutput::Net(NetOutgoing::TunBind { fd }));
        }
        Self {
            node_id: cfg.node_id,
            worker: cfg.worker,
            data_plane: DataPlane::new(cfg.node_id, cfg.services),
            backend_udp_slot: 0,
            timer: TimePivot::build(),
            #[cfg(feature = "vpn")]
            backend_tun_slot: 0,
            queue,
        }
    }

    fn convert_output<'a>(&mut self, _now: Instant, output: DataPlaneOutput<'a, SE, TC>) -> Option<TaskOutput<'a, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>> {
        match output {
            DataPlaneOutput::Net(NetOutput::UdpPacket(to, buf)) => Some(TaskOutput::Net(NetOutgoing::UdpPacket {
                slot: self.backend_udp_slot,
                to,
                data: convert_buf1(buf),
            })),
            #[cfg(feature = "vpn")]
            DataPlaneOutput::Net(NetOutput::TunPacket(buf)) => Some(TaskOutput::Net(NetOutgoing::TunPacket {
                slot: self.backend_tun_slot,
                data: convert_buf1(buf),
            })),
            #[cfg(not(feature = "vpn"))]
            DataPlaneOutput::Net(NetOutput::TunPacket(_)) => None,
            DataPlaneOutput::Net(NetOutput::UdpPackets(to, buf)) => Some(TaskOutput::Net(NetOutgoing::UdpPackets {
                slot: self.backend_udp_slot,
                to,
                data: convert_buf1(buf),
            })),
            DataPlaneOutput::Control(bus) => Some(TaskOutput::Bus(BusEvent::ChannelPublish((), true, bus))),
            DataPlaneOutput::ShutdownResponse => {
                self.queue.push_back(TaskOutput::Net(NetOutgoing::UdpUnlisten { slot: self.backend_udp_slot }));
                self.queue.push_back(TaskOutput::Bus(BusEvent::ChannelUnsubscribe(ChannelIn::Broadcast)));
                self.queue.push_back(TaskOutput::Bus(BusEvent::ChannelUnsubscribe(ChannelIn::Worker(self.worker))));
                self.queue.push_back(TaskOutput::Destroy);
                self.queue.pop_front()
            }
            DataPlaneOutput::Ext(ext) => Some(TaskOutput::Ext(ext)),
            DataPlaneOutput::Continue => None,
        }
    }

    fn try_process_output<'a>(&mut self, now: Instant, output: DataPlaneOutput<'a, SE, TC>) -> Option<TaskOutput<'a, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>> {
        let out = self.convert_output(now, output);
        if out.is_some() {
            return out;
        }
        self.pop_output_direct(now)
    }

    fn pop_output_direct<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>> {
        // self.pop_output_direct(now)
        let now_ms = self.timer.timestamp_ms(now);
        loop {
            let output = self.data_plane.pop_output(now_ms)?;
            let out = self.convert_output(now, output);
            if out.is_some() {
                return out;
            }
        }
    }
}

impl<SC, SE, TC: Debug, TW: Debug> Task<(), ExtOut<SE>, ChannelIn, ChannelOut, EventIn<TW>, EventOut<TC>> for DataPlaneTask<SC, SE, TC, TW> {
    /// The type identifier for the task.
    const TYPE: u16 = 1;

    fn on_tick<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>> {
        if let Some(out) = self.queue.pop_front() {
            return Some(out);
        }

        let now_ms = self.timer.timestamp_ms(now);
        self.data_plane.on_tick(now_ms);
        self.pop_output(now)
    }

    fn on_event<'a>(&mut self, now: Instant, input: TaskInput<'a, (), ChannelIn, EventIn<TW>>) -> Option<TaskOutput<'a, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>> {
        match input {
            TaskInput::Ext(_) => None,
            TaskInput::Net(net) => match net {
                NetIncoming::UdpListenResult { bind, result } => {
                    let res = result.expect("Should to bind UDP socket");
                    self.backend_udp_slot = res.1;
                    log::info!("Data plane task bound udp {} to {}", bind, res.0);
                    None
                }
                NetIncoming::UdpPacket { slot: _, from, data } => {
                    let now_ms = self.timer.timestamp_ms(now);
                    let out = self.data_plane.on_event(now_ms, DataPlaneInput::Net(NetInput::UdpPacket(from, data.into())))?;
                    self.try_process_output(now, out)
                }
                #[cfg(feature = "vpn")]
                NetIncoming::TunBindResult { result } => {
                    let res = result.expect("Should to bind TUN device");
                    self.backend_tun_slot = res;
                    log::info!("Data plane task bound tun to {}", res);
                    None
                }
                #[cfg(feature = "vpn")]
                NetIncoming::TunPacket { slot: _, data } => {
                    let now_ms = self.timer.timestamp_ms(now);
                    let out = self.data_plane.on_event(now_ms, DataPlaneInput::Net(NetInput::TunPacket(data.into())))?;
                    self.try_process_output(now, out)
                }
                #[cfg(not(feature = "vpn"))]
                NetIncoming::TunBindResult { .. } => None,
                #[cfg(not(feature = "vpn"))]
                NetIncoming::TunPacket { .. } => None,
            },
            TaskInput::Bus(_, event) => {
                let output = self.data_plane.on_event(self.timer.timestamp_ms(now), DataPlaneInput::Event(event))?;
                self.try_process_output(now, output)
            }
        }
    }

    fn pop_output<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>> {
        if let Some(output) = self.queue.pop_front() {
            return Some(output);
        }

        self.pop_output_direct(now)
    }

    fn shutdown<'a>(&mut self, now: Instant) -> Option<TaskOutput<'a, ExtOut<SE>, ChannelIn, ChannelOut, EventOut<TC>>> {
        let output = self.data_plane.on_event(self.timer.timestamp_ms(now), DataPlaneInput::ShutdownRequest)?;
        self.try_process_output(now, output)
    }
}

fn convert_buf1<'a>(buf1: GenericBuffer<'a>) -> Buffer<'a> {
    match buf1 {
        GenericBuffer::Vec(buf) => Buffer::Vec(buf),
        GenericBuffer::Ref(buf) => Buffer::Ref(buf),
    }
}

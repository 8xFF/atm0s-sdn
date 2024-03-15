use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Instant,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::data_plane::DataPlane;
use sans_io_runtime::{bus::BusEvent, Buffer, NetIncoming, NetOutgoing, Task, TaskInput, TaskOutput};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ChannelIn {
    Broadcast,
    Worker(u16),
}

pub type ChannelOut = ();

#[derive(Debug, Clone)]
pub enum EventIn {}

#[derive(Debug, Clone)]
pub enum EventOut {}

pub struct DataPlaneCfg {
    pub worker: u16,
    pub node_id: NodeId,
    pub port: u16,
    #[cfg(feature = "vpn")]
    pub vpn_tun_fd: sans_io_runtime::backend::tun::TunFd,
}

pub struct DataPlaneTask<TC, TW> {
    #[allow(unused)]
    node_id: NodeId,
    worker: u16,
    data_plane: DataPlane<TC, TW>,
    backend_udp_slot: usize,
    #[cfg(feature = "vpn")]
    backend_tun_slot: usize,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
}

impl<TC, TW> DataPlaneTask<TC, TW> {
    pub fn build(cfg: DataPlaneCfg) -> Self {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), cfg.port));
        Self {
            node_id: cfg.node_id,
            worker: cfg.worker,
            data_plane: DataPlane::new(cfg.node_id),
            backend_udp_slot: 0,
            #[cfg(feature = "vpn")]
            backend_tun_slot: 0,
            queue: VecDeque::from([
                TaskOutput::Net(NetOutgoing::UdpListen { addr, reuse: true }),
                #[cfg(feature = "vpn")]
                TaskOutput::Net(NetOutgoing::TunBind { fd: cfg.vpn_tun_fd }),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Broadcast)),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Worker(cfg.worker))),
            ]),
        }
    }
}

impl<TC, TW> Task<ChannelIn, ChannelOut, EventIn, EventOut> for DataPlaneTask<TC, TW> {
    /// The type identifier for the task.
    const TYPE: u16 = 1;

    fn on_tick<'a>(&mut self, _now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }

    fn on_event<'a>(&mut self, _now: Instant, input: TaskInput<'a, ChannelIn, EventIn>) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        match input {
            TaskInput::Net(net) => match net {
                NetIncoming::UdpListenResult { bind, result } => {
                    let res = result.expect("Should to bind UDP socket");
                    self.backend_udp_slot = res.1;
                    log::info!("Data plane task bound {} to {}", bind, res.0);
                    None
                }
                NetIncoming::UdpPacket { slot: _, from, data } => todo!(),
                #[cfg(feature = "vpn")]
                NetIncoming::TunBindResult { result } => {
                    let res = result.expect("Should to bind TUN device");
                    self.backend_tun_slot = res;
                    log::info!("Data plane task bound to {}", res);
                    None
                }
                #[cfg(feature = "vpn")]
                NetIncoming::TunPacket { slot: _, data } => self.process_incoming_tun(data),
            },
            TaskInput::Bus(_, event) => {
                todo!()
            }
        }
    }

    fn pop_output<'a>(&mut self, _now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.pop_front()
    }

    fn shutdown<'a>(&mut self, _now: Instant) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        self.queue.push_back(TaskOutput::Net(NetOutgoing::UdpUnlisten { slot: self.backend_udp_slot }));
        self.queue.push_back(TaskOutput::Bus(BusEvent::ChannelUnsubscribe(ChannelIn::Broadcast)));
        self.queue.push_back(TaskOutput::Bus(BusEvent::ChannelUnsubscribe(ChannelIn::Worker(self.worker))));
        None
    }
}

#[cfg(feature = "vpn")]
fn rewrite_tun_pkt(payload: &mut [u8]) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        payload[2] = 0;
        payload[3] = 2;
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        payload[2] = 8;
        payload[3] = 0;
    }
}

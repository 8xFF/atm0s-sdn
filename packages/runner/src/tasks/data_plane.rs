use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Instant,
};

use atm0s_sdn_identity::NodeId;
use atm0s_sdn_network::{event::DataEvent, msg::TransportMsgHeader};
use atm0s_sdn_router::shadow::{ShadowRouter, ShadowRouterDelta};
use atm0s_sdn_router::{RouteAction, RouterTable};
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

pub struct DataPlaneCfg {
    pub worker: u16,
    pub node_id: NodeId,
    pub port: u16,
    #[cfg(feature = "vpn")]
    pub vpn_tun_fd: sans_io_runtime::backend::tun::TunFd,
}

pub struct DataPlaneTask {
    #[allow(unused)]
    node_id: NodeId,
    worker: u16,
    backend_udp_slot: usize,
    #[cfg(feature = "vpn")]
    backend_tun_slot: usize,
    router: ShadowRouter<SocketAddr>,
    queue: VecDeque<TaskOutput<'static, ChannelIn, ChannelOut, EventOut>>,
}

impl DataPlaneTask {
    pub fn build(cfg: DataPlaneCfg) -> Self {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), cfg.port));
        Self {
            node_id: cfg.node_id,
            worker: cfg.worker,
            backend_udp_slot: 0,
            #[cfg(feature = "vpn")]
            backend_tun_slot: 0,
            router: ShadowRouter::new(cfg.node_id),
            queue: VecDeque::from([
                TaskOutput::Net(NetOutgoing::UdpListen { addr, reuse: true }),
                #[cfg(feature = "vpn")]
                TaskOutput::Net(NetOutgoing::TunBind { fd: cfg.vpn_tun_fd }),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Broadcast)),
                TaskOutput::Bus(BusEvent::ChannelSubscribe(ChannelIn::Worker(cfg.worker))),
            ]),
        }
    }

    /// This function is used to route the message to the next hop,
    /// This is stateless and doing across workers
    fn process_service_msg<'a>(&self, from: SocketAddr, buf: &'a mut [u8]) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        let header = if let Some(header) = DataEvent::is_network(buf) {
            header
        } else {
            let data = DataEvent::try_from(&buf[..]).ok()?;
            log::trace!("Received from remote {}", from);
            return Some(TaskOutput::Bus(BusEvent::ChannelPublish((), true, EventOut::Data(from, data))));
        };
        let next = self.router.derive_action(&header.route, header.to_service_id);
        match next {
            RouteAction::Local => {
                #[cfg(feature = "vpn")]
                {
                    const SERVICE_TYPE: u8 = atm0s_sdn_network::controller_plane::core_services::vpn::SERVICE_TYPE;
                    if header.to_service_id == SERVICE_TYPE {
                        //for vpn service, direct sending to tun socket
                        rewrite_tun_pkt(&mut buf[header.serialize_size()..]);
                        return Some(TaskOutput::Net(NetOutgoing::TunPacket {
                            slot: self.backend_tun_slot,
                            data: Buffer::Ref(&buf[header.serialize_size()..]),
                        }));
                    }
                }

                let data = DataEvent::try_from(&buf[..]).ok()?;
                log::trace!("Received from remote {}", from);
                Some(TaskOutput::Bus(BusEvent::ChannelPublish((), true, EventOut::Data(from, data))))
            }
            RouteAction::Reject => None,
            RouteAction::Next(next) => {
                TransportMsgHeader::decrease_ttl(buf);
                log::trace!("Forward to next {}", next);
                Some(TaskOutput::Net(NetOutgoing::UdpPacket {
                    slot: self.backend_udp_slot,
                    to: next,
                    data: Buffer::Ref(buf),
                }))
            }
            RouteAction::Broadcast(_local, remotes) => {
                log::trace!("Forward to next remotes {:?}", remotes);
                TransportMsgHeader::decrease_ttl(buf);
                Some(TaskOutput::Net(NetOutgoing::UdpPackets {
                    slot: self.backend_udp_slot,
                    to: remotes,
                    data: Buffer::Ref(buf),
                }))
            }
        }
    }

    #[cfg(feature = "vpn")]
    fn process_incoming_tun<'a>(&self, buf: &'a mut [u8]) -> Option<TaskOutput<'a, ChannelIn, ChannelOut, EventOut>> {
        use atm0s_sdn_identity::NodeIdType;
        use atm0s_sdn_network::msg::TransportMsg;
        use atm0s_sdn_router::RouteRule;

        let to_ip = &buf[20..24];
        let dest = NodeId::build(self.node_id.geo1(), self.node_id.geo2(), self.node_id.group(), to_ip[3]);
        if dest == self.node_id {
            //This is for me, just rewrite
            Some(TaskOutput::Net(NetOutgoing::TunPacket {
                slot: self.backend_tun_slot,
                data: Buffer::Ref(buf),
            }))
        } else {
            match self.router.path_to_node(dest) {
                RouteAction::Next(remote) => {
                    //TODO decrease TTL
                    const SERVICE_TYPE: u8 = atm0s_sdn_network::controller_plane::core_services::vpn::SERVICE_TYPE;
                    let msg = TransportMsg::build(SERVICE_TYPE, SERVICE_TYPE, RouteRule::ToNode(dest), 0, 0, buf);
                    Some(TaskOutput::Net(NetOutgoing::UdpPacket {
                        slot: self.backend_udp_slot,
                        to: remote,
                        data: Buffer::Vec(msg.take()),
                    }))
                }
                _ => None,
            }
        }
    }
}

impl Task<ChannelIn, ChannelOut, EventIn, EventOut> for DataPlaneTask {
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
                NetIncoming::UdpPacket { slot: _, from, data } => self.process_service_msg(from, data),
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
            TaskInput::Bus(_, EventIn::Data(remote, msg)) => {
                log::trace!("Sending to remote {:?}", msg);
                Some(TaskOutput::Net(NetOutgoing::UdpPacket {
                    slot: self.backend_udp_slot,
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
